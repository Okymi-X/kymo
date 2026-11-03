//! The whole VPN design against real containers: certificates, profiles, the gateway, two lab
//! networks, two players and the firewall between them. Skipped unless these are set:
//!
//!   KYMO_TEST_GATEWAY_IMAGE  the image built from deploy/vpn-gateway
//!   KYMO_TEST_IMAGE          a challenge image that serves /flag.txt on port 8000
//!   KYMO_TEST_CLI            `docker` (default) or `podman`, used to run the player machines
//!   DOCKER_HOST              the socket, when it is not the default one
//!
//! Example (rootless Podman):
//!   DOCKER_HOST=unix:///run/user/1000/podman/podman.sock KYMO_TEST_CLI=podman \
//!   KYMO_TEST_GATEWAY_IMAGE=localhost/kymo/vpn-gateway:1 KYMO_TEST_IMAGE=localhost/kymo-lab/flag-server:1 \
//!   cargo test -p kymo-vpn --features docker --test gateway -- --nocapture

#![cfg(feature = "docker")]

use std::net::Ipv4Addr;
use std::path::Path;
use std::time::{Duration, Instant};

use kymo_instances::docker::DockerRuntime;
use kymo_instances::{InstanceRuntime, InstanceSpec, Placement, StartRequest};
use kymo_vpn::docker::DockerGateway;
use kymo_vpn::{
    Allow, Ca, ClientProfile, Gateway, GatewayConfig, Ipv4Net, LabPool, Revoked, TunnelPool,
    client_config, client_profile, firewall_script, generate_tls_crypt_key, server_conf,
};
use tokio::process::Command;

const NOW: i64 = 1_800_000_000;

struct Env {
    cli: String,
    gateway_image: String,
    challenge_image: String,
    tag: String,
}

async fn cli(env: &Env, args: &[&str]) -> (bool, String) {
    let out = Command::new(&env.cli)
        .args(args)
        .output()
        .await
        .expect("run the container CLI");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text.trim().to_owned())
}

async fn wait_for<F, Fut>(what: &str, secs: u64, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("timed out waiting for {what}");
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

struct Player {
    name: &'static str,
    tunnel: Ipv4Addr,
    container: String,
    lab: Ipv4Net,
    lab_network: String,
    flag_ip: Ipv4Addr,
}

#[tokio::test]
async fn players_reach_only_their_own_lab_through_their_own_tunnel() {
    let (Ok(gateway_image), Ok(challenge_image)) = (
        std::env::var("KYMO_TEST_GATEWAY_IMAGE"),
        std::env::var("KYMO_TEST_IMAGE"),
    ) else {
        eprintln!(
            "skipped: set KYMO_TEST_GATEWAY_IMAGE and KYMO_TEST_IMAGE to run against a daemon"
        );
        return;
    };
    let env = Env {
        cli: std::env::var("KYMO_TEST_CLI").unwrap_or_else(|_| "docker".into()),
        gateway_image,
        challenge_image,
        tag: format!("{}", std::process::id()),
    };
    let result = tokio::spawn(scenario(Env {
        cli: env.cli.clone(),
        gateway_image: env.gateway_image.clone(),
        challenge_image: env.challenge_image.clone(),
        tag: env.tag.clone(),
    }))
    .await;
    cleanup(&env).await;
    if let Err(e) = result {
        std::panic::resume_unwind(e.into_panic());
    }
}

async fn cleanup(env: &Env) {
    let t = &env.tag;
    for name in [
        format!("kymo-test-alice-{t}"),
        format!("kymo-test-bob-{t}"),
        format!("kymo-test-gw-{t}"),
    ] {
        cli(env, &["rm", "-f", &name]).await;
    }
    let runtime = DockerRuntime::connect("127.0.0.1").unwrap();
    for (_, c) in runtime
        .list_managed()
        .await
        .unwrap_or_default()
        .iter()
        .map(|m| ((), m.handle.clone()))
    {
        if c.len() > 3 {
            let _ = runtime.stop(&c).await;
        }
    }
    for lab in ["a", "b"] {
        cli(env, &["network", "rm", &format!("kymo-test-lab-{lab}-{t}")]).await;
    }
}

async fn scenario(env: Env) {
    let t = env.tag.clone();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("gateway");
    let tunnel = TunnelPool::new("10.8.200.0/24".parse().unwrap()).unwrap();
    let labs = LabPool::new("10.77.200.0/24".parse().unwrap(), 27).unwrap();

    // 1. The authority, the gateway's own certificate, and everything the gateway reads.
    let ca = Ca::load(Ca::generate(NOW, 3650).unwrap()).unwrap();
    // Use the real clock for validity so the client machines accept the certificates.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let ca = {
        let _ = ca;
        Ca::load(Ca::generate(now, 30).unwrap()).unwrap()
    };
    let server = ca.issue_server("kymo-gateway", now, 30).unwrap();
    let ta = generate_tls_crypt_key();
    write(&root.join("pki/ca.crt"), ca.cert_pem());
    write(&root.join("pki/server.crt"), &server.cert_pem);
    write(&root.join("pki/server.key"), &server.key_pem);
    write(&root.join("pki/ta.key"), &ta);
    write(&root.join("pki/crl.pem"), &ca.crl(&[], 1, now, 30).unwrap());
    write(
        &root.join("server.conf"),
        &server_conf(&tunnel, &labs.pool()),
    );
    std::fs::create_dir_all(root.join("ccd")).unwrap();

    // 2. Two players, each with their own lab network and challenge container.
    let spec = InstanceSpec {
        image: env.challenge_image.clone(),
        port: 8000,
        cpu_millis: 250,
        memory_mb: 64,
        pids: 64,
        ttl_secs: 600,
        env: vec![],
        read_only: true,
    };
    let runtime = DockerRuntime::connect("127.0.0.1").expect("connect to the daemon");
    let gateway = DockerGateway::connect().expect("connect to the daemon");
    gateway
        .ensure_running(&GatewayConfig {
            name: format!("kymo-test-gw-{t}"),
            image: env.gateway_image.clone(),
            dir: root.clone(),
            publish_ip: "127.0.0.1".into(),
            publish_port: 0,
            bind_options: std::env::var("KYMO_TEST_BIND_OPTIONS").unwrap_or_default(),
        })
        .await
        .expect("start the gateway");

    let mut players = Vec::new();
    for (i, name) in ["alice", "bob"].into_iter().enumerate() {
        let lab = labs.lab(i as u32).unwrap();
        let lab_network = format!("kymo-test-lab-{}-{t}", if i == 0 { "a" } else { "b" });
        runtime
            .ensure_lab(&lab_network, &lab.to_string())
            .await
            .expect("make the lab");
        gateway
            .attach(&lab_network)
            .await
            .expect("attach the gateway");
        let flag_ip = labs.instance_slots(&lab)[0];
        let started = runtime
            .start(&StartRequest {
                instance_id: format!("test{t}{i}"),
                spec: spec.clone(),
                placement: Placement::Lab {
                    network: lab_network.clone(),
                    ip: flag_ip.to_string(),
                },
            })
            .await
            .expect("start the challenge in the lab");
        assert_eq!(started.ip.as_deref(), Some(flag_ip.to_string().as_str()));
        players.push(Player {
            name,
            tunnel: tunnel.client(i as u32).unwrap(),
            container: format!("kymo-test-{name}-{t}"),
            lab,
            lab_network,
            flag_ip,
        });
    }
    let status = gateway.status().await.unwrap();
    assert!(status.running);
    for p in &players {
        assert!(
            status.networks.contains(&p.lab_network),
            "attached to {}: {:?}",
            p.lab_network,
            status.networks
        );
    }

    // 3. Each player's address file and the firewall.
    for p in &players {
        write(
            &root.join("ccd").join(p.name),
            &client_config(p.tunnel, &tunnel),
        );
    }
    let allows: Vec<Allow> = players
        .iter()
        .map(|p| Allow {
            client: p.tunnel,
            lab: p.lab,
        })
        .collect();
    write(
        &root.join("rules.sh"),
        &firewall_script(&allows, &tunnel.net(), &labs.pool()),
    );
    gateway.apply_rules().await.expect("apply the firewall");

    // 4. The player machines: separate containers running the downloaded profile.
    let (_, gw_ip) = cli(
        &env,
        &[
            "inspect",
            "-f",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}",
            &format!("kymo-test-gw-{t}"),
        ],
    )
    .await;
    let gw_ip = gw_ip
        .split_whitespace()
        .find(|s| s.parse::<Ipv4Addr>().is_ok())
        .expect("the gateway has an address")
        .to_owned();
    let mut issued = Vec::new();
    for p in &players {
        let cert = ca.issue_client(p.name, now, 30).unwrap();
        let profile = client_profile(&ClientProfile {
            remote_host: &gw_ip,
            remote_port: 1194,
            ca_pem: ca.cert_pem(),
            cert_pem: &cert.cert_pem,
            key_pem: &cert.key_pem,
            tls_crypt_key: &ta,
        })
        .unwrap();
        let file = dir.path().join(format!("{}.ovpn", p.name));
        write(&file, &profile);
        let bind = if std::env::var("KYMO_TEST_BIND_OPTIONS").is_ok() {
            ":ro,z"
        } else {
            ":ro"
        };
        let (ok, out) = cli(
            &env,
            &[
                "run",
                "-d",
                "--name",
                &p.container,
                "--cap-add",
                "NET_ADMIN",
                "--device",
                "/dev/net/tun",
                "-v",
                &format!("{}:/c.ovpn{bind}", file.display()),
                "--entrypoint",
                "openvpn",
                &env.gateway_image,
                "--config",
                "/c.ovpn",
                // These player machines are containers, which cannot use the kernel fast path
                // (DCO). A real player's machine is not affected, so profiles do not set this.
                "--disable-dco",
            ],
        )
        .await;
        assert!(ok, "start the {} machine: {out}", p.name);
        issued.push(cert);
    }
    for p in &players {
        let mut up = false;
        for _ in 0..60 {
            let probe = cli(
                &env,
                &["exec", &p.container, "ip", "-br", "addr", "show", "tun0"],
            )
            .await;
            if probe.1.contains("10.8.200.") {
                up = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if !up {
            let (_, client_log) = cli(&env, &["logs", "--tail", "25", &p.container]).await;
            let (_, gateway_log) = cli(
                &env,
                &["logs", "--tail", "25", &format!("kymo-test-gw-{t}")],
            )
            .await;
            panic!(
                "{}'s tunnel did not come up\n--- client ---\n{client_log}\n--- gateway ---\n{gateway_log}",
                p.name
            );
        }
        let (_, addr) = cli(
            &env,
            &["exec", &p.container, "ip", "-br", "addr", "show", "tun0"],
        )
        .await;
        assert!(
            addr.contains(&p.tunnel.to_string()),
            "{} should get {}: {addr}",
            p.name,
            p.tunnel
        );
        // A real player is on another machine: remove the direct path so only the tunnel remains.
        cli(
            &env,
            &["exec", &p.container, "ip", "route", "del", "default"],
        )
        .await;
    }

    // 5. Reach and isolation.
    let fetch = |from: &str, ip: Ipv4Addr| {
        let from = from.to_owned();
        let env = &env;
        async move {
            cli(
                env,
                &[
                    "exec",
                    &from,
                    "wget",
                    "-q",
                    "-T",
                    "4",
                    "-O-",
                    &format!("http://{ip}:8000/flag.txt"),
                ],
            )
            .await
        }
    };
    let (alice, bob) = (&players[0], &players[1]);
    for p in [alice, bob] {
        let (ok, body) = fetch(&p.container, p.flag_ip).await;
        assert!(
            ok && body.contains("KYMO{"),
            "{} reaches their own lab: {body}",
            p.name
        );
    }
    for (from, to) in [(alice, bob), (bob, alice)] {
        let (ok, body) = fetch(&from.container, to.flag_ip).await;
        assert!(
            !ok,
            "{} must not reach {}'s lab, got: {body}",
            from.name, to.name
        );
        let (ping, _) = cli(
            &env,
            &[
                "exec",
                &from.container,
                "ping",
                "-c",
                "1",
                "-W",
                "2",
                &to.tunnel.to_string(),
            ],
        )
        .await;
        assert!(
            !ping,
            "{} must not reach {}'s tunnel address",
            from.name, to.name
        );
        let (ping, _) = cli(
            &env,
            &[
                "exec",
                &from.container,
                "ping",
                "-c",
                "1",
                "-W",
                "2",
                "1.1.1.1",
            ],
        )
        .await;
        assert!(!ping, "the tunnel is not a route to the internet");
    }
    // Lab containers cannot reach each other's labs or the internet either.
    let lab_a = format!("kymo-test{t}0");
    let (ok, _) = cli(
        &env,
        &[
            "exec",
            &lab_a,
            "wget",
            "-q",
            "-T",
            "3",
            "-O-",
            &format!("http://{}:8000/flag.txt", bob.flag_ip),
        ],
    )
    .await;
    assert!(!ok, "one lab must not reach another");
    let (ok, _) = cli(
        &env,
        &[
            "exec",
            &lab_a,
            "wget",
            "-q",
            "-T",
            "3",
            "-O-",
            "http://1.1.1.1",
        ],
    )
    .await;
    assert!(!ok, "a lab has no internet");

    // 6. Revoking alice: her address file goes, her session is dropped, her certificate is
    //    listed, and she cannot come back. Bob is not affected.
    std::fs::remove_file(root.join("ccd").join("alice")).unwrap();
    write(
        &root.join("pki/crl.pem"),
        &ca.crl(
            &[Revoked {
                serial: issued[0].serial.clone(),
                at: now,
            }],
            2,
            now,
            30,
        )
        .unwrap(),
    );
    write(
        &root.join("rules.sh"),
        &firewall_script(&allows[1..], &tunnel.net(), &labs.pool()),
    );
    gateway.apply_rules().await.expect("apply the firewall");
    gateway.kick("alice").await.expect("drop alice's session");
    let alice_name = alice.container.clone();
    let env_ref = &env;
    wait_for("alice to be refused", 40, || async {
        cli(
            env_ref,
            &[
                "ps",
                "-q",
                "--filter",
                &format!("name={alice_name}"),
                "--filter",
                "status=running",
            ],
        )
        .await
        .1
        .is_empty()
    })
    .await;
    let (ok, body) = fetch(&bob.container, bob.flag_ip).await;
    assert!(ok && body.contains("KYMO{"), "bob keeps working: {body}");

    // 7. Tear down in the right order: gateway leaves, then the lab network goes.
    for p in &players {
        gateway.detach(&p.lab_network).await.unwrap();
    }
    let after = gateway.status().await.unwrap().networks;
    assert!(
        players.iter().all(|p| !after.contains(&p.lab_network)),
        "detached: {after:?}"
    );
}
