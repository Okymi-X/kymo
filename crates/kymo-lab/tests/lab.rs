//! The lab host and its client, end to end over real sockets (plain and TLS on loopback), with
//! the in memory runtime standing in for Docker.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use kymo_instances::{
    FakeRuntime, InstanceRuntime, InstanceSpec, Placement, RuntimeError, RuntimeStatus,
    StartRequest,
};
use kymo_lab::manifest::Manifest;
use kymo_lab::server::{self, Cidr, Lab, LabConfig, Mode};
use kymo_lab::{CreateRequest, RemoteRuntime, tls};

const TOKEN: &str = "test-token-with-enough-length-123456";
const IMAGE: &str = "lab/web:1";

struct Fixture {
    url: String,
    runtime: Arc<FakeRuntime>,
    lab: Arc<Lab>,
    clock: Arc<AtomicI64>,
}

fn spec() -> InstanceSpec {
    InstanceSpec {
        image: IMAGE.into(),
        port: 8000,
        cpu_millis: 500,
        memory_mb: 256,
        pids: 128,
        ttl_secs: 1800,
        env: vec![],
        read_only: true,
    }
}

fn manifest() -> Manifest {
    serde_json::from_str(&format!(
        r#"{{"artifact":"{IMAGE}","port":8000,"env_allow":["FLAG"],"max_ttl_secs":900}}"#
    ))
    .unwrap()
}

fn config() -> LabConfig {
    let mut cfg = LabConfig::new("test-lab", TOKEN);
    cfg.manifests = vec![manifest()];
    cfg
}

async fn start_with(
    cfg: LabConfig,
    runtime: Arc<FakeRuntime>,
    acceptor: Option<tokio_rustls::TlsAcceptor>,
) -> Fixture {
    let clock = Arc::new(AtomicI64::new(1_800_000_000));
    let c = Arc::clone(&clock);
    let lab = Lab::with_clock(
        cfg,
        runtime.clone(),
        Arc::new(move || c.load(Ordering::SeqCst)),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(server::serve(
        listener,
        server::router(Arc::clone(&lab)),
        acceptor.clone(),
        std::future::pending(),
    ));
    let scheme = if acceptor.is_some() { "https" } else { "http" };
    Fixture {
        url: format!("{scheme}://127.0.0.1:{port}"),
        runtime,
        lab,
        clock,
    }
}

async fn start(cfg: LabConfig) -> Fixture {
    start_with(cfg, Arc::new(FakeRuntime::new()), None).await
}

fn client(f: &Fixture) -> RemoteRuntime {
    RemoteRuntime::new(&f.url, TOKEN, None, "127.0.0.1").unwrap()
}

fn request(id: &str, name: &str) -> StartRequest {
    StartRequest {
        instance_id: id.into(),
        spec: spec(),
        placement: Placement::Published,
        name: Some(name.into()),
        meta: vec![("event".into(), "1".into()), ("owner".into(), "7".into())],
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test]
async fn an_instance_is_started_listed_inspected_and_removed_through_the_api() {
    let f = start(config()).await;
    let lab = client(&f);
    let started = lab
        .start(&request("11", "kymo-c1-u7-aaaaaa"))
        .await
        .unwrap();
    assert!(started.host_port > 0);
    assert_eq!(
        lab.status(&started.handle).await.unwrap(),
        RuntimeStatus::Running
    );
    assert_eq!(f.runtime.running_count(), 1);

    let managed = lab.list_managed().await.unwrap();
    assert_eq!(managed.len(), 1);
    assert_eq!(managed[0].instance_id, "11");

    let info = lab.instances().await.unwrap().remove(0);
    assert_eq!(info.name, "kymo-c1-u7-aaaaaa");
    assert_eq!(info.meta.get("owner").map(String::as_str), Some("7"));
    // The runtime really got the name and metadata the platform built.
    let seen = f.runtime.spec_of(&started.handle).unwrap();
    assert_eq!(seen.port, 8000);

    lab.stop(&started.handle).await.unwrap();
    assert_eq!(
        lab.status(&started.handle).await.unwrap(),
        RuntimeStatus::Missing
    );
    lab.stop(&started.handle)
        .await
        .expect("removing a missing instance succeeds");
    assert_eq!(f.runtime.running_count(), 0);
}

#[tokio::test]
async fn the_token_and_the_source_address_are_enforced() {
    let f = start(config()).await;
    // The liveness probe needs no token, everything else does.
    assert_eq!(
        http()
            .get(format!("{}/healthz", f.url))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        http()
            .get(format!("{}/v1/health", f.url))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let bad = RemoteRuntime::new(&f.url, "wrong-token-wrong-token-wrong-1", None, "h").unwrap();
    assert!(
        matches!(bad.health().await, Err(RuntimeError::Unavailable(m)) if m.contains("refused"))
    );
    assert_eq!(client(&f).health().await.unwrap().name, "test-lab");

    let mut only_elsewhere = config();
    only_elsewhere.allow_from = vec![Cidr::parse("10.0.0.0/8").unwrap()];
    let g = start(only_elsewhere).await;
    assert!(
        client(&g).health().await.is_err(),
        "loopback is not in 10.0.0.0/8"
    );
    let mut loopback = config();
    loopback.allow_from = vec![
        Cidr::parse("127.0.0.0/8").unwrap(),
        Cidr::parse("::1").unwrap(),
    ];
    assert!(client(&start(loopback).await).health().await.is_ok());
}

#[tokio::test]
async fn repeated_wrong_tokens_are_throttled_even_for_the_right_one() {
    let f = start(config()).await;
    for _ in 0..10 {
        assert_eq!(
            http()
                .get(format!("{}/v1/health", f.url))
                .bearer_auth("nope")
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
    assert_eq!(
        http()
            .get(format!("{}/v1/health", f.url))
            .bearer_auth("nope")
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    assert_eq!(
        http()
            .get(format!("{}/v1/health", f.url))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        429,
        "the address is locked out for a minute"
    );
}

#[tokio::test]
async fn the_lab_host_does_not_trust_what_the_platform_sends() {
    let f = start(config()).await;
    let lab = client(&f);
    let refused = |r: Result<kymo_instances::Started, RuntimeError>| match r {
        Err(RuntimeError::Failed(m)) => m,
        other => panic!("expected a refusal, got {other:?}"),
    };
    // An image with no manifest.
    let mut other = request("1", "kymo-a-b-cccccc");
    other.spec.image = "evil/miner:latest".into();
    assert!(refused(lab.start(&other).await).contains("no manifest"));
    // Names: wrong prefix, bad characters, forged paths.
    for name in ["other-c1-u7-aaaaaa", "kymo-C1", "kymo-../etc", "kymo-a b"] {
        assert!(
            !refused(lab.start(&request("2", name)).await).is_empty(),
            "{name}"
        );
    }
    // Environment the manifest does not allow.
    let mut env = request("3", "kymo-c1-u7-bbbbbb");
    env.spec.env = vec![("SECRET".into(), "x".into())];
    assert!(refused(lab.start(&env).await).contains("SECRET"));
    // Allowed environment passes, and the manifest decides port and time.
    let mut ok = request("4", "kymo-c1-u7-cccccc");
    ok.spec.env = vec![("FLAG".into(), "KYMO{x}".into())];
    ok.spec.port = 9999;
    ok.spec.ttl_secs = 99_999;
    let started = lab.start(&ok).await.unwrap();
    let seen = f.runtime.spec_of(&started.handle).unwrap();
    assert_eq!(seen.port, 8000, "the platform cannot pick the port");
    assert_eq!(seen.ttl_secs, 900, "nor raise the time limit");
    assert!(seen.read_only);
    // A lab host with no gateway of its own cannot place an instance on a private network.
    let mut vpn = request("5", "kymo-c1-u7-dddddd");
    vpn.placement = Placement::Lab {
        network: "kymo-lab-1".into(),
        ip: "10.77.0.42".into(),
        gateway: Some("10.77.0.33".into()),
    };
    assert!(refused(lab.start(&vpn).await).contains("VPN"));
}

#[tokio::test]
async fn open_mode_runs_any_valid_image_within_the_caps() {
    let mut cfg = LabConfig::new("open-lab", TOKEN);
    cfg.mode = Mode::Open;
    cfg.caps.max_memory_mb = 128;
    let f = start(cfg).await;
    let lab = client(&f);
    let mut any = request("1", "kymo-c1-u7-aaaaaa");
    any.spec.image = "anything/goes:2".into();
    any.spec.memory_mb = 64;
    assert!(lab.start(&any).await.is_ok());
    let mut big = request("2", "kymo-c1-u7-bbbbbb");
    big.spec.image = "anything/goes:2".into();
    big.spec.memory_mb = 4096;
    assert!(
        matches!(lab.start(&big).await, Err(RuntimeError::Failed(m)) if m.contains("memory_mb"))
    );
    let mut bad_image = request("3", "kymo-c1-u7-cccccc");
    bad_image.spec.image = "-rm -rf".into();
    assert!(lab.start(&bad_image).await.is_err());
}

#[tokio::test]
async fn names_are_unique_retries_are_safe_and_capacity_is_enforced() {
    let mut cfg = config();
    cfg.max_instances = 2;
    let f = start(cfg).await;
    let lab = client(&f);
    let first = lab.start(&request("1", "kymo-c1-u7-aaaaaa")).await.unwrap();
    // The platform retrying a create whose answer it lost gets the same instance back.
    let again = lab.start(&request("1", "kymo-c1-u7-aaaaaa")).await.unwrap();
    assert_eq!(first.handle, again.handle);
    assert_eq!(f.runtime.running_count(), 1);
    // The same name for a different run is a conflict, as is the same run under another name.
    assert!(
        matches!(lab.start(&request("2", "kymo-c1-u7-aaaaaa")).await, Err(RuntimeError::Failed(m)) if m.contains("in use"))
    );
    assert!(
        matches!(lab.start(&request("1", "kymo-c1-u7-bbbbbb")).await, Err(RuntimeError::Failed(m)) if m.contains("already exists"))
    );
    lab.start(&request("2", "kymo-c1-u7-bbbbbb")).await.unwrap();
    // Full: a 503, which the platform reads as "try again later".
    assert!(
        matches!(lab.start(&request("3", "kymo-c1-u7-cccccc")).await, Err(RuntimeError::Unavailable(m)) if m.contains("full"))
    );
    assert_eq!(lab.health().await.unwrap().running, 2);
}

#[tokio::test]
async fn expiry_is_enforced_by_the_lab_host_itself() {
    let f = start(config()).await;
    let lab = client(&f);
    let started = lab.start(&request("1", "kymo-c1-u7-aaaaaa")).await.unwrap();
    f.clock.fetch_add(500, Ordering::SeqCst);
    assert!(f.lab.cleanup().await.is_empty(), "not yet");
    f.clock.fetch_add(500, Ordering::SeqCst); // past the 900 second manifest limit
    let removed = f.lab.cleanup().await;
    assert_eq!(removed, vec!["kymo-c1-u7-aaaaaa".to_owned()]);
    assert_eq!(f.runtime.running_count(), 0);
    assert_eq!(
        lab.status(&started.handle).await.unwrap(),
        RuntimeStatus::Missing
    );
}

#[tokio::test]
async fn reconcile_removes_what_the_platform_no_longer_lists_but_spares_new_starts() {
    let f = start(config()).await;
    let lab = client(&f);
    lab.start(&request("10", "kymo-c1-u1-aaaaaa"))
        .await
        .unwrap();
    lab.start(&request("11", "kymo-c1-u2-bbbbbb"))
        .await
        .unwrap();
    // Just created: inside the grace period nothing is touched, so a start in progress is safe.
    assert!(lab.reconcile(vec![], Some(60)).await.unwrap().is_empty());
    f.clock.fetch_add(120, Ordering::SeqCst);
    let removed = lab.reconcile(vec!["10".into()], Some(60)).await.unwrap();
    assert_eq!(removed, vec!["kymo-c1-u2-bbbbbb".to_owned()]);
    assert_eq!(f.runtime.running_count(), 1);
    // A container the lab never recorded (left by a crash) goes too.
    f.runtime.add_orphan("99", spec());
    let removed = lab.reconcile(vec!["10".into()], Some(0)).await.unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(f.runtime.running_count(), 1);
}

#[tokio::test]
async fn restart_recreates_under_the_same_name() {
    let f = start(config()).await;
    let lab = client(&f);
    let first = lab.start(&request("1", "kymo-c1-u7-aaaaaa")).await.unwrap();
    let response = http()
        .post(format!("{}/v1/instances/kymo-c1-u7-aaaaaa/restart", f.url))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let info: kymo_lab::InstanceInfo = response.json().await.unwrap();
    assert_eq!(info.name, "kymo-c1-u7-aaaaaa");
    assert_ne!(info.id, first.handle, "a new container");
    assert_eq!(f.runtime.running_count(), 1);
    assert_eq!(
        http()
            .post(format!("{}/v1/instances/nothing/restart", f.url))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

#[tokio::test]
async fn the_instance_list_survives_a_restart_of_the_lab_host() {
    let dir = std::env::temp_dir().join(format!("kymo-lab-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = Arc::new(FakeRuntime::new());
    let mut cfg = config();
    cfg.state_path = Some(dir.join("instances.json"));
    let f = start_with(cfg, Arc::clone(&runtime), None).await;
    client(&f)
        .start(&request("1", "kymo-c1-u7-aaaaaa"))
        .await
        .unwrap();

    // A new process: same runtime (the containers are still there), same state file.
    let mut cfg2 = config();
    cfg2.state_path = Some(dir.join("instances.json"));
    let g = start_with(cfg2, Arc::clone(&runtime), None).await;
    g.lab.load().await;
    let listed = client(&g).instances().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "kymo-c1-u7-aaaaaa");
    assert_eq!(listed[0].meta.get("owner").map(String::as_str), Some("7"));
    // And a record whose container vanished meanwhile is dropped on load.
    runtime.stop(&listed[0].id).await.unwrap();
    let h = start_with(
        {
            let mut c = config();
            c.state_path = Some(dir.join("instances.json"));
            c
        },
        runtime,
        None,
    )
    .await;
    h.lab.load().await;
    assert!(client(&h).instances().await.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn health_artifacts_and_the_action_log_are_readable() {
    let f = start(config()).await;
    let lab = client(&f);
    let health = lab.health().await.unwrap();
    assert_eq!(
        (health.mode.as_str(), health.artifacts, health.api.as_str()),
        ("manifest", 1, "v1")
    );
    assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    let artifacts = lab.artifacts().await.unwrap();
    assert_eq!(
        (artifacts[0].artifact.as_str(), artifacts[0].port),
        (IMAGE, 8000)
    );
    lab.start(&request("1", "kymo-c1-u7-aaaaaa")).await.unwrap();
    let log = lab.log().await.unwrap();
    assert_eq!(log[0].action, "create");
    assert!(log[0].detail.contains("owner=7"), "{}", log[0].detail);
}

#[tokio::test]
async fn reachable_connects_the_way_a_player_would() {
    let f = start(config()).await;
    let lab = client(&f);
    let open = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = open.local_addr().unwrap().port();
    assert!(lab.reachable(port).await);
    drop(open);
    assert!(!lab.reachable(port).await);
}

#[tokio::test]
async fn tls_with_a_pinned_fingerprint_works_and_anything_else_does_not() {
    let dir = std::env::temp_dir().join(format!("kymo-lab-pin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let identity = tls::load_or_create(&dir, &[]).unwrap();
    let f = start_with(
        config(),
        Arc::new(FakeRuntime::new()),
        Some(tls::acceptor(&identity).unwrap()),
    )
    .await;

    let pinned =
        RemoteRuntime::new(&f.url, TOKEN, Some(&identity.fingerprint), "127.0.0.1").unwrap();
    assert_eq!(pinned.health().await.unwrap().name, "test-lab");
    pinned
        .start(&request("1", "kymo-c1-u7-aaaaaa"))
        .await
        .expect("a full call works over TLS");

    // Another certificate's fingerprint: the handshake is refused before any token is sent.
    let other = tls::load_or_create(&dir.join("other"), &[]).unwrap();
    assert_ne!(other.fingerprint, identity.fingerprint);
    let wrong = RemoteRuntime::new(&f.url, TOKEN, Some(&other.fingerprint), "127.0.0.1").unwrap();
    assert!(matches!(
        wrong.health().await,
        Err(RuntimeError::Unavailable(_))
    ));
    // No pin: a self-signed certificate is not trusted by the usual authorities.
    let unpinned = RemoteRuntime::new(&f.url, TOKEN, None, "127.0.0.1").unwrap();
    assert!(unpinned.health().await.is_err());
    // Plain HTTP to a TLS port goes nowhere.
    let plain =
        RemoteRuntime::new(&f.url.replace("https", "http"), TOKEN, None, "127.0.0.1").unwrap();
    assert!(plain.health().await.is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_wire_format_is_the_documented_one() {
    let f = start(config()).await;
    let body = CreateRequest {
        name: "kymo-c1-u7-aaaaaa".into(),
        instance_id: "1".into(),
        spec: spec(),
        meta: BTreeMap::new(),
        lab: None,
        flags: Vec::new(),
    };
    let response = http()
        .post(format!("{}/v1/instances", f.url))
        .bearer_auth(TOKEN)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let json: serde_json::Value = response.json().await.unwrap();
    for key in [
        "id",
        "name",
        "instance_id",
        "status",
        "host_port",
        "created_at",
        "expires_at",
    ] {
        assert!(json.get(key).is_some(), "missing {key} in {json}");
    }
    assert_eq!(json["status"], "running");
    let missing = http()
        .get(format!("{}/v1/instances/nope", f.url))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    assert!(missing.json::<serde_json::Value>().await.unwrap()["error"].is_string());
    assert_eq!(
        http()
            .delete(format!("{}/v1/instances/nope", f.url))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
}

// ---------- the lab host's own VPN gateway ----------

mod vpn {
    use super::*;
    use kymo_lab::client::RemoteGateway;

    fn refused<T>(result: Result<T, RuntimeError>) -> String {
        match result {
            Ok(_) => panic!("expected a refusal"),
            Err(e) => e.to_string(),
        }
    }
    use kymo_lab::vpn::LabVpn;
    use kymo_vpn::{FakeGateway, Gateway, GatewayConfig};

    struct Vpn {
        f: Fixture,
        gateway: Arc<FakeGateway>,
        remote: RemoteGateway,
        client: Arc<RemoteRuntime>,
        lab_dir: std::path::PathBuf,
        staging: std::path::PathBuf,
    }

    async fn vpn_lab(only: bool) -> Vpn {
        let runtime = Arc::new(FakeRuntime::new());
        let f = start_with(config(), runtime.clone(), None).await;
        let gateway = Arc::new(FakeGateway::new());
        let root = std::env::temp_dir().join(format!(
            "kymo-lab-vpn-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        let lab_dir = root.join("lab");
        let staging = root.join("platform");
        std::fs::create_dir_all(&lab_dir).unwrap();
        std::fs::create_dir_all(staging.join("pki")).unwrap();
        std::fs::create_dir_all(staging.join("ccd")).unwrap();
        let config = GatewayConfig {
            name: "gw".into(),
            image: "img".into(),
            dir: lab_dir.clone(),
            publish_ip: "0.0.0.0".into(),
            publish_port: 1194,
            bind_options: String::new(),
        };
        f.lab.set_vpn(
            Arc::new(LabVpn::new(
                gateway.clone(),
                runtime,
                config,
                "lab.example.org".into(),
                "10.77.0.0/16".parse().unwrap(),
            )),
            only,
        );
        let client = Arc::new(RemoteRuntime::new(&f.url, TOKEN, None, "127.0.0.1").unwrap());
        Vpn {
            remote: RemoteGateway::new(Arc::clone(&client)),
            client,
            f,
            gateway,
            lab_dir,
            staging,
        }
    }

    fn rand_suffix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::from(d.subsec_nanos()))
    }

    fn lab_request(id: &str, name: &str, ip: &str) -> StartRequest {
        let mut r = request(id, name);
        r.placement = Placement::Lab {
            network: "kymo-lab-1".into(),
            ip: ip.into(),
            gateway: Some("10.77.0.33".into()),
        };
        r
    }

    #[tokio::test]
    async fn health_tells_the_platform_where_players_connect() {
        let v = vpn_lab(true).await;
        let health = v.client.health().await.unwrap();
        let info = health.vpn.unwrap();
        assert_eq!(
            (info.host.as_str(), info.port, info.pool.as_str()),
            ("lab.example.org", 1194, "10.77.0.0/16")
        );
        assert!(health.vpn_only);
    }

    #[tokio::test]
    async fn a_vpn_only_lab_host_never_publishes_a_port() {
        let v = vpn_lab(true).await;
        let published = v.client.start(&request("1", "kymo-c1-u7-aaaaaa")).await;
        assert!(refused(published).contains("only runs instances on a private VPN network"));
        assert_eq!(v.f.runtime.running_count(), 0);

        v.client
            .ensure_lab("kymo-lab-1", "10.77.0.32/27")
            .await
            .unwrap();
        let started = v
            .client
            .start(&lab_request("2", "kymo-c1-u7-bbbbbb", "10.77.0.42"))
            .await
            .unwrap();
        assert_eq!(started.ip.as_deref(), Some("10.77.0.42"));
        let placed = v.f.runtime.placement_of(&started.handle).unwrap();
        assert!(matches!(placed, Placement::Lab { ref network, .. } if network == "kymo-lab-1"));
    }

    #[tokio::test]
    async fn addresses_outside_the_vpn_range_are_refused() {
        let v = vpn_lab(true).await;
        v.client
            .ensure_lab("kymo-lab-1", "10.77.0.32/27")
            .await
            .unwrap();
        for (ip, bind) in [
            ("192.168.1.5", "10.77.0.33"),
            ("10.77.0.42", "0.0.0.0"),
            ("10.77.0.42", "127.0.0.1"),
        ] {
            let mut r = lab_request("3", "kymo-c1-u7-cccccc", ip);
            r.placement = Placement::Lab {
                network: "kymo-lab-1".into(),
                ip: ip.into(),
                gateway: Some(bind.into()),
            };
            let message = refused(v.client.start(&r).await);
            assert!(
                message.contains("outside this lab host's VPN range"),
                "{ip} {bind}: {message}"
            );
        }
        assert!(v.client.ensure_lab("bridge", "10.77.1.0/27").await.is_err());
        assert!(
            v.client
                .ensure_lab("kymo-lab-2", "172.16.0.0/27")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn the_platform_drives_the_gateway_through_the_api() {
        let v = vpn_lab(true).await;
        for (path, text) in [
            ("pki/ca.crt", "ca"),
            ("pki/server.crt", "crt"),
            ("pki/server.key", "key"),
            ("pki/ta.key", "ta"),
            ("pki/crl.pem", "crl"),
            ("server.conf", "conf"),
            ("rules.sh", "rules"),
            ("ccd/u7", "ifconfig-push 10.8.0.2 255.255.0.0"),
        ] {
            std::fs::write(v.staging.join(path), text).unwrap();
        }
        let config = GatewayConfig {
            name: "gw".into(),
            image: String::new(),
            dir: v.staging.clone(),
            publish_ip: String::new(),
            publish_port: 1194,
            bind_options: String::new(),
        };
        v.remote.ensure_running(&config).await.unwrap();
        // The files crossed over, the secret ones readable by the owner only.
        assert_eq!(
            std::fs::read_to_string(v.lab_dir.join("ccd/u7")).unwrap(),
            "ifconfig-push 10.8.0.2 255.255.0.0"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(v.lab_dir.join("pki/server.key"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o077,
                0,
                "the private key must not be readable by others"
            );
        }
        v.client
            .ensure_lab("kymo-lab-1", "10.77.0.32/27")
            .await
            .unwrap();
        v.remote.attach("kymo-lab-1").await.unwrap();
        v.remote.apply_rules().await.unwrap();
        v.remote.kick("u7").await.unwrap();
        assert_eq!(v.gateway.networks(), vec!["kymo-lab-1".to_owned()]);
        assert_eq!(v.gateway.rules_runs(), 1);
        assert_eq!(v.gateway.kicked(), vec!["u7".to_owned()]);
        assert!(v.remote.status().await.unwrap().running);

        // A player the platform no longer lists loses the file that lets them in.
        std::fs::remove_file(v.staging.join("ccd/u7")).unwrap();
        v.remote.ensure_running(&config).await.unwrap();
        assert!(!v.lab_dir.join("ccd/u7").exists());

        // Only addresses of lab networks can be probed, so this is no port scanner.
        assert!(
            v.remote
                .reachable("10.77.0.42".parse().unwrap(), 80)
                .await
                .unwrap()
        );
        assert!(
            v.remote
                .reachable("192.168.1.1".parse().unwrap(), 22)
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(v.lab_dir.parent().unwrap());
    }

    #[tokio::test]
    async fn a_lab_host_without_a_gateway_says_so() {
        let f = start(config()).await;
        let client = RemoteRuntime::new(&f.url, TOKEN, None, "127.0.0.1").unwrap();
        let health = client.health().await.unwrap();
        assert!(health.vpn.is_none() && !health.vpn_only);
        let message = client
            .ensure_lab("kymo-lab-1", "10.77.0.32/27")
            .await
            .unwrap_err()
            .to_string();
        assert!(message.contains("no VPN gateway"), "{message}");
    }
}
