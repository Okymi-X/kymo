//! Runs the Docker runtime against a real daemon (Docker, or rootless Podman through its
//! Docker compatible socket). Skipped unless `KYMO_TEST_IMAGE` names an image whose service
//! answers `GET /flag.txt` on `KYMO_TEST_PORT` (default 8000). Point `DOCKER_HOST` at the
//! socket when it is not the default:
//!
//!   DOCKER_HOST=unix:///run/user/1000/podman/podman.sock \
//!   KYMO_TEST_IMAGE=localhost/kymo-lab/flag-server:1 cargo test -p kymo-instances --features docker --test docker

#![cfg(feature = "docker")]

use std::time::Duration;

use kymo_instances::docker::DockerRuntime;
use kymo_instances::{InstanceRuntime, InstanceSpec, RuntimeStatus, StartRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn get(port: u16, path: &str) -> String {
    let mut last = String::new();
    for _ in 0..30 {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)).await {
            let request = format!("GET {path} HTTP/1.0\r\nHost: lab\r\n\r\n");
            if stream.write_all(request.as_bytes()).await.is_ok() {
                let mut body = String::new();
                if stream.read_to_string(&mut body).await.is_ok() && !body.is_empty() {
                    return body;
                }
            }
        }
        last = format!("no answer on port {port}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    last
}

#[tokio::test]
async fn a_real_container_starts_answers_isolated_and_is_removed() {
    let Ok(image) = std::env::var("KYMO_TEST_IMAGE") else {
        eprintln!("skipped: set KYMO_TEST_IMAGE to run against a daemon");
        return;
    };
    let port: u16 = std::env::var("KYMO_TEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000);
    let runtime = DockerRuntime::connect("127.0.0.1").expect("connect to the daemon");
    let spec = InstanceSpec {
        image,
        port,
        cpu_millis: 250,
        memory_mb: 64,
        pids: 64,
        ttl_secs: 120,
        env: vec![("KYMO_TEST".into(), "1".into())],
        read_only: true,
    };
    let id = format!("it{}", std::process::id());
    let request = StartRequest::new(id.clone(), spec);

    let started = runtime.start(&request).await.expect("start");
    assert!(started.host_port > 0);
    assert_eq!(
        runtime.status(&started.handle).await.unwrap(),
        RuntimeStatus::Running
    );

    let page = get(started.host_port, "/flag.txt").await;
    assert!(
        page.contains("KYMO{"),
        "the service should answer with its flag: {page}"
    );

    let managed = runtime.list_managed().await.unwrap();
    assert!(
        managed
            .iter()
            .any(|m| m.handle == started.handle && m.instance_id == id),
        "the container is found again by its labels"
    );

    runtime.stop(&started.handle).await.expect("stop");
    assert_eq!(
        runtime.status(&started.handle).await.unwrap(),
        RuntimeStatus::Missing
    );
    assert!(
        runtime
            .list_managed()
            .await
            .unwrap()
            .iter()
            .all(|m| m.handle != started.handle)
    );
    runtime
        .stop(&started.handle)
        .await
        .expect("stopping twice is fine");

    // The per instance network goes with the container.
    let again = runtime
        .start(&request)
        .await
        .expect("the same instance id can start again");
    runtime.stop(&again.handle).await.unwrap();
}
