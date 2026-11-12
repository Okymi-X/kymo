//! Flags on the lab host: a container gets its flag as an environment variable the manifest
//! names, anything the manifest has no place for is refused, and no API answer or log line ever
//! carries a flag value.

use std::sync::Arc;
use std::sync::atomic::AtomicI64;

use kymo_instances::{
    FakeRuntime, FlagDelivery, InstanceRuntime, InstanceSpec, Placement, RunFlag, StartExtras,
    StartRequest,
};
use kymo_lab::RemoteRuntime;
use kymo_lab::manifest::Manifest;
use kymo_lab::server::{self, Lab, LabConfig};

const TOKEN: &str = "test-token-with-enough-length-123456";
const IMAGE: &str = "lab/box:1";
const SECRET: &str = "KYMO{box_9f3a1c7d}";

struct Fixture {
    url: String,
    runtime: Arc<FakeRuntime>,
    lab: Arc<Lab>,
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

fn manifests(extra: &str) -> Vec<Manifest> {
    let json = format!(r#"{{"artifact":"{IMAGE}","port":8000,"max_ttl_secs":900{extra}}}"#);
    vec![serde_json::from_str(&json).unwrap()]
}

async fn start(manifests: Vec<Manifest>) -> Fixture {
    let mut cfg = LabConfig::new("test-lab", TOKEN);
    cfg.manifests = manifests;
    let runtime = Arc::new(FakeRuntime::new());
    let clock = Arc::new(AtomicI64::new(1_800_000_000));
    let lab = Lab::with_clock(
        cfg,
        runtime.clone(),
        Arc::new(move || clock.load(std::sync::atomic::Ordering::SeqCst)),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(server::serve(
        listener,
        server::router(Arc::clone(&lab)),
        None,
        std::future::pending(),
    ));
    Fixture {
        url: format!("http://127.0.0.1:{port}"),
        runtime,
        lab,
    }
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
        meta: vec![("owner".into(), "user:7".into())],
    }
}

fn flag(name: &str, deliver: FlagDelivery) -> RunFlag {
    RunFlag {
        name: name.into(),
        value: SECRET.into(),
        deliver,
    }
}

fn extras(name: &str) -> StartExtras {
    StartExtras {
        flags: vec![flag(name, FlagDelivery::Inject)],
        verifier_ip: None,
    }
}

const WITH_INJECT: &str = r#","inject":{"flags":[{"name":"flag","env":"FLAG"}]}"#;

#[tokio::test]
async fn a_container_gets_its_flag_as_the_environment_variable_the_manifest_names() {
    let f = start(manifests(WITH_INJECT)).await;
    let lab = client(&f);
    let out = lab
        .start_with(&request("1", "kymo-c1-u7-aaaaaa"), &extras("flag"))
        .await
        .unwrap();
    let env = f.runtime.spec_of(&out.started.handle).unwrap().env;
    assert!(
        env.contains(&("FLAG".to_owned(), SECRET.to_owned())),
        "{env:?}"
    );
}

#[tokio::test]
async fn a_flag_with_no_place_in_the_manifest_is_refused_and_nothing_starts() {
    let f = start(manifests(WITH_INJECT)).await;
    let lab = client(&f);
    let err = lab
        .start_with(&request("1", "kymo-c1-u7-aaaaaa"), &extras("typo"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("typo"), "{err}");
    assert_eq!(f.runtime.running_count(), 0);
    // No manifest block at all: flags are refused too.
    let bare = start(manifests("")).await;
    assert!(
        client(&bare)
            .start_with(&request("1", "kymo-c1-u7-aaaaaa"), &extras("flag"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn no_answer_and_no_log_line_carries_a_flag_value() {
    let f = start(manifests(WITH_INJECT)).await;
    let lab = client(&f);
    lab.start_with(&request("1", "kymo-c1-u7-aaaaaa"), &extras("flag"))
        .await
        .unwrap();
    let http = reqwest::Client::new();
    for path in [
        "/v1/instances",
        "/v1/instances/kymo-c1-u7-aaaaaa",
        "/v1/log",
        "/v1/health",
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
        assert!(!body.contains("9f3a1c7d"), "{path} leaked the flag: {body}");
    }
}

#[tokio::test]
async fn a_lab_side_restart_hands_out_the_same_flag_again() {
    let f = start(manifests(WITH_INJECT)).await;
    let lab = client(&f);
    let out = lab
        .start_with(&request("1", "kymo-c1-u7-aaaaaa"), &extras("flag"))
        .await
        .unwrap();
    let again = f.lab.restart("kymo-c1-u7-aaaaaa").await.unwrap();
    assert_ne!(again.id, out.started.handle, "a new container");
    let env = f.runtime.spec_of(&again.id).unwrap().env;
    assert!(env.contains(&("FLAG".to_owned(), SECRET.to_owned())));
}

#[tokio::test]
async fn the_verifier_and_its_address_must_agree_with_the_manifest() {
    let f = start(manifests(WITH_INJECT)).await;
    let lab = client(&f);
    // An address for a Verifier the manifest does not have.
    let mut req = request("1", "kymo-c1-u7-aaaaaa");
    req.placement = Placement::Lab {
        network: "kymo-lab-1".into(),
        ip: "10.77.0.42".into(),
        gateway: Some("10.77.0.33".into()),
    };
    let held = StartExtras {
        flags: vec![flag("flag", FlagDelivery::Hold)],
        verifier_ip: Some("10.77.0.43".into()),
    };
    assert!(lab.start_with(&req, &held).await.is_err());
    assert_eq!(f.runtime.running_count(), 0);
}

#[tokio::test]
async fn an_ordinary_instance_is_unchanged() {
    let f = start(manifests("")).await;
    let lab = client(&f);
    let out = lab.start(&request("1", "kymo-c1-u7-aaaaaa")).await.unwrap();
    assert!(out.host_port > 0);
    assert!(f.runtime.spec_of(&out.handle).unwrap().env.is_empty());
}
