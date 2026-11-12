//! The Verifier of a Responder instance: started next to the machine on the same lab network at
//! the address the platform allocated, reported as an endpoint, removed with its instance, and
//! holding the flag that no API answer ever shows.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use kymo_instances::{
    FakeRuntime, FlagDelivery, InstanceRuntime, InstanceSpec, Placement, RunFlag, StartExtras,
    StartRequest,
};
use kymo_lab::RemoteRuntime;
use kymo_lab::manifest::Manifest;
use kymo_lab::server::{self, Lab, LabConfig};
use kymo_lab::vpn::LabVpn;
use kymo_vpn::{FakeGateway, GatewayConfig};

const TOKEN: &str = "test-token-with-enough-length-123456";
const IMAGE: &str = "lab/box:1";
const SECRET: &str = "KYMO{box_9f3a1c7d}";

struct Fixture {
    url: String,
    runtime: Arc<FakeRuntime>,
    lab: Arc<Lab>,
    client: RemoteRuntime,
    _dir: tempfile::TempDir,
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

async fn start(with_definition: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    if with_definition {
        let def = dir.path().join("verifiers/box");
        std::fs::create_dir_all(&def).unwrap();
        std::fs::write(def.join("verifier.json"), "{}").unwrap();
    }
    let manifest: Manifest = serde_json::from_str(&format!(
        r#"{{"artifact":"{IMAGE}","port":8000,"max_ttl_secs":900,
            "verifier":{{"definition":"box","port":8080}}}}"#
    ))
    .unwrap();
    let mut cfg = LabConfig::new("test-lab", TOKEN);
    cfg.manifests = vec![manifest];
    cfg.verifiers_dir = dir.path().join("verifiers");
    cfg.verifier_health_check = false;
    let runtime = Arc::new(FakeRuntime::new());
    let clock = Arc::new(AtomicI64::new(1_800_000_000));
    let lab = Lab::with_clock(
        cfg,
        runtime.clone(),
        Arc::new(move || clock.load(Ordering::SeqCst)),
    );
    let gateway = Arc::new(FakeGateway::new());
    let config = GatewayConfig {
        name: "gw".into(),
        image: "img".into(),
        dir: dir.path().join("gateway"),
        publish_ip: "0.0.0.0".into(),
        publish_port: 1194,
        bind_options: String::new(),
    };
    lab.set_vpn(
        Arc::new(LabVpn::new(
            gateway,
            runtime.clone(),
            config,
            "lab.example.org".into(),
            "10.77.0.0/16".parse().unwrap(),
        )),
        true,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(server::serve(
        listener,
        server::router(Arc::clone(&lab)),
        None,
        std::future::pending(),
    ));
    let url = format!("http://127.0.0.1:{port}");
    let client = RemoteRuntime::new(&url, TOKEN, None, "127.0.0.1").unwrap();
    client
        .ensure_lab("kymo-lab-1", "10.77.0.32/27")
        .await
        .unwrap();
    Fixture {
        url,
        runtime,
        lab,
        client,
        _dir: dir,
    }
}

fn request(name: &str, ip: &str) -> StartRequest {
    StartRequest {
        instance_id: "1".into(),
        spec: spec(),
        placement: Placement::Lab {
            network: "kymo-lab-1".into(),
            ip: ip.into(),
            gateway: Some("10.77.0.33".into()),
        },
        name: Some(name.into()),
        meta: vec![("owner".into(), "user:7".into())],
    }
}

fn held(verifier_ip: &str) -> StartExtras {
    StartExtras {
        flags: vec![RunFlag {
            name: "flag".into(),
            value: SECRET.into(),
            deliver: FlagDelivery::Hold,
        }],
        verifier_ip: Some(verifier_ip.into()),
    }
}

#[tokio::test]
async fn a_responder_gets_its_verifier_on_the_same_network_and_reports_both_endpoints() {
    let f = start(true).await;
    let out = f
        .client
        .start_with(
            &request("kymo-c1-u7-aaaaaa", "10.77.0.42"),
            &held("10.77.0.43"),
        )
        .await
        .unwrap();
    let verifiers = f.runtime.verifiers();
    assert_eq!(verifiers.len(), 1);
    let (_, v) = &verifiers[0];
    assert_eq!(
        (v.network.as_str(), v.ip.as_str(), v.port),
        ("kymo-lab-1", "10.77.0.43", 8080)
    );
    assert_eq!(v.name, "kymo-c1-u7-aaaaaa-v");
    assert!(v.definition_dir.ends_with("verifiers/box"));
    assert!(
        v.run_json.contains(SECRET),
        "the Verifier is given the held flag"
    );
    assert!(
        v.run_json.contains("10.77.0.42"),
        "and the machine's address"
    );
    let roles: Vec<(&str, &str, Option<u16>)> = out
        .endpoints
        .iter()
        .map(|e| (e.role.as_str(), e.host.as_str(), e.port))
        .collect();
    assert_eq!(
        roles,
        vec![
            ("machine", "10.77.0.42", None),
            ("verifier", "10.77.0.43", Some(8080))
        ]
    );
    assert_eq!(out.endpoints[1].scheme.as_deref(), Some("http"));
}

#[tokio::test]
async fn no_answer_shows_the_held_flag() {
    let f = start(true).await;
    f.client
        .start_with(
            &request("kymo-c1-u7-aaaaaa", "10.77.0.42"),
            &held("10.77.0.43"),
        )
        .await
        .unwrap();
    let http = reqwest::Client::new();
    for path in [
        "/v1/instances",
        "/v1/instances/kymo-c1-u7-aaaaaa",
        "/v1/log",
    ] {
        let body = http
            .get(format!("{}{path}", f.url))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!body.contains("9f3a1c7d"), "{path}: {body}");
        assert!(!body.contains("admin_token"), "{path}: {body}");
    }
}

#[tokio::test]
async fn the_verifier_goes_with_its_instance() {
    let f = start(true).await;
    let out = f
        .client
        .start_with(
            &request("kymo-c1-u7-aaaaaa", "10.77.0.42"),
            &held("10.77.0.43"),
        )
        .await
        .unwrap();
    assert_eq!(f.runtime.verifiers().len(), 1);
    f.client.stop(&out.started.handle).await.unwrap();
    assert_eq!(f.runtime.running_count(), 0);
    assert!(
        f.runtime.verifiers().is_empty(),
        "removed with the instance"
    );
}

#[tokio::test]
async fn a_lab_side_restart_replaces_the_verifier_too() {
    let f = start(true).await;
    f.client
        .start_with(
            &request("kymo-c1-u7-aaaaaa", "10.77.0.42"),
            &held("10.77.0.43"),
        )
        .await
        .unwrap();
    let before = f.runtime.verifiers()[0].0.clone();
    f.lab.restart("kymo-c1-u7-aaaaaa").await.unwrap();
    let after = f.runtime.verifiers();
    assert_eq!(after.len(), 1, "one Verifier, not two");
    assert_ne!(after[0].0, before);
    assert!(
        after[0].1.run_json.contains(SECRET),
        "the same flag is held again"
    );
}

#[tokio::test]
async fn a_missing_definition_or_a_bad_address_refuses_the_start_and_leaves_nothing() {
    let f = start(false).await;
    let err = f
        .client
        .start_with(
            &request("kymo-c1-u7-aaaaaa", "10.77.0.42"),
            &held("10.77.0.43"),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("definition"), "{err}");
    assert_eq!(f.runtime.running_count(), 0, "the machine was taken back");
    assert!(f.runtime.verifiers().is_empty());

    let ok = start(true).await;
    for bad in ["192.168.1.5", "10.77.0.42", "not-an-ip"] {
        let err = ok
            .client
            .start_with(&request("kymo-c1-u7-bbbbbb", "10.77.0.42"), &held(bad))
            .await;
        assert!(err.is_err(), "{bad}");
    }
    assert_eq!(ok.runtime.running_count(), 0);
}

#[tokio::test]
async fn a_verifier_without_an_instance_is_swept() {
    let f = start(true).await;
    f.client
        .start_with(
            &request("kymo-c1-u7-aaaaaa", "10.77.0.42"),
            &held("10.77.0.43"),
        )
        .await
        .unwrap();
    // Forget the instance behind the lab host's back: only the Verifier is left.
    let handle = f.runtime.list_handles()[0].clone();
    f.runtime.stop(&handle).await.unwrap();
    f.lab.cleanup().await;
    assert!(f.runtime.verifiers().is_empty());
}
