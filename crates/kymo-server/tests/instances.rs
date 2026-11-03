use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_instances::{FakeRuntime, Policy, ResourceCaps};
use kymo_server::state::InstanceSetup;
use kymo_server::{AppState, db, instances, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;
const FLAG: &str = "KYMO{one}";

struct World {
    app: Router,
    clock: Arc<AtomicI64>,
}

impl World {
    fn set_time(&self, t: i64) {
        self.clock.store(t, Ordering::SeqCst);
    }
}

async fn world(runtime: Option<(Arc<FakeRuntime>, Policy)>) -> (World, AppState) {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let clock = Arc::new(AtomicI64::new(T0));
    let shared = Arc::clone(&clock);
    let mut state = AppState::new(pool, PASSWORD)
        .unwrap()
        .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
    if let Some((fake, policy)) = runtime {
        state = state.with_instances(InstanceSetup {
            runtime: fake,
            policy,
            caps: ResourceCaps::default(),
            public_host: "play.example.org".into(),
            reap_grace_secs: 5,
        });
    }
    (
        World {
            app: router(state.clone()),
            clock,
        },
        state,
    )
}

struct Reply {
    status: StatusCode,
    bytes: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    content_type: Option<&str>,
    body: Vec<u8>,
    ip: Option<&str>,
) -> Reply {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    if let Some(ct) = content_type {
        req = req.header("content-type", ct);
    }
    if let Some(ip) = ip {
        let addr: SocketAddr = format!("{ip}:4000").parse().unwrap();
        req = req.extension(ConnectInfo(addr));
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Reply { status, bytes }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (ct, bytes) = match body {
        Some(v) => (Some("application/json"), v.to_string().into_bytes()),
        None => (None, Vec::new()),
    };
    let r = send(app, method, uri, token, ct, bytes, None).await;
    (r.status, r.json())
}

async fn admin_token(app: &Router) -> String {
    let (status, body) = call(
        app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "password": PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body["token"].as_str().unwrap().to_owned()
}

/// Registers a player and returns (token, user id).
async fn player(app: &Router, name: &str) -> (String, i64) {
    let (status, body) = call(
        app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({ "username": name, "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    (
        body["token"].as_str().unwrap().to_owned(),
        body["user"]["id"].as_i64().unwrap(),
    )
}

fn spec(ttl: i64) -> Value {
    json!({
        "image": "registry.example.org/ctf/web-1:1.0",
        "port": 8000,
        "cpu_millis": 500,
        "memory_mb": 256,
        "pids": 128,
        "ttl_secs": ttl,
        "env": [["SECRET_SEED", "do-not-leak"]],
    })
}

struct Ctx {
    w: World,
    state: AppState,
    fake: Arc<FakeRuntime>,
    admin: String,
    event_id: i64,
    slug: String,
    category: i64,
}

impl Ctx {
    async fn new(policy: Policy) -> Ctx {
        let fake = Arc::new(FakeRuntime::new());
        let (w, state) = world(Some((Arc::clone(&fake), policy))).await;
        Ctx::build(w, state, fake).await
    }

    async fn without_runtime() -> Ctx {
        let (w, state) = world(None).await;
        Ctx::build(w, state, Arc::new(FakeRuntime::new())).await
    }

    async fn build(w: World, state: AppState, fake: Arc<FakeRuntime>) -> Ctx {
        let admin = admin_token(&w.app).await;
        let (status, event) = call(
            &w.app,
            "POST",
            "/api/v1/admin/events",
            Some(&admin),
            Some(json!({ "name": "Spring CTF", "kind": "jeopardy", "mode": "solo" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{event}");
        let event_id = event["id"].as_i64().unwrap();
        let (status, category) = call(
            &w.app,
            "POST",
            &format!("/api/v1/admin/events/{event_id}/categories"),
            Some(&admin),
            Some(json!({ "name": "Web" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{category}");
        let ctx = Ctx {
            w,
            state,
            fake,
            admin,
            event_id,
            slug: event["slug"].as_str().unwrap().to_owned(),
            category: category["id"].as_i64().unwrap(),
        };
        ctx.set_state("registration_open").await;
        ctx
    }

    async fn set_state(&self, state: &str) {
        let (status, body) = self
            .admin(
                "POST",
                &format!("/admin/events/{}/state", self.event_id),
                Some(json!({ "state": state })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    async fn admin(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        call(
            &self.w.app,
            method,
            &format!("/api/v1{uri}"),
            Some(&self.admin),
            body,
        )
        .await
    }

    /// A published challenge; `extra` overrides the default body.
    async fn challenge(&self, extra: Value) -> i64 {
        let mut body = json!({
            "category_id": self.category,
            "title": "Box",
            "description": "connect to it",
            "scoring": { "kind": "static", "points": 100 },
            "flag": { "kind": "static", "value": FLAG },
            "requires_instance": true,
            "instance_config": spec(600),
        });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (status, created) = self
            .admin(
                "POST",
                &format!("/admin/events/{}/challenges", self.event_id),
                Some(body),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let id = created["id"].as_i64().unwrap();
        let (status, body) = self
            .admin("POST", &format!("/admin/challenges/{id}/publish"), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        id
    }

    async fn register(&self, token: &str) {
        let (status, body) = call(
            &self.w.app,
            "POST",
            &format!("/api/v1/events/{}/register", self.slug),
            Some(token),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    async fn act(
        &self,
        token: &str,
        method: &str,
        challenge: i64,
        tail: &str,
    ) -> (StatusCode, Value) {
        call(
            &self.w.app,
            method,
            &format!(
                "/api/v1/events/{}/challenges/{challenge}/instance{tail}",
                self.slug
            ),
            Some(token),
            None,
        )
        .await
    }

    /// A registered player in a live event.
    async fn live_player(&self, name: &str) -> String {
        let (token, _) = player(&self.w.app, name).await;
        self.register(&token).await;
        token
    }
}

fn roomy() -> Policy {
    Policy {
        restart_cooldown_secs: 30,
        max_starts: 5,
        max_concurrent: 1,
    }
}

#[tokio::test]
async fn instances_are_unavailable_without_a_runtime() {
    let c = Ctx::without_runtime().await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    let (status, body) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}

#[tokio::test]
async fn start_show_and_stop_never_leak_the_container_setup() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;

    let (status, none) = c.act(&alice, "GET", id, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(none["state"], "none");

    let (status, started) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    assert_eq!(started["state"], "running");
    assert_eq!(started["host"], "play.example.org");
    assert!(started["port"].as_i64().unwrap() > 0);
    assert_eq!(started["remaining_secs"], 600);
    let text = started.to_string();
    for secret in [
        "do-not-leak",
        "registry.example.org",
        "SECRET_SEED",
        "fake-",
    ] {
        assert!(!text.contains(secret), "leaked {secret}: {text}");
    }
    assert_eq!(c.fake.running_count(), 1);
    let handle = c.fake.list_handles()[0].clone();
    let limits = c.fake.spec_of(&handle).unwrap();
    assert_eq!(
        (limits.cpu_millis, limits.memory_mb, limits.pids),
        (500, 256, 128)
    );

    let (status, stopped) = c.act(&alice, "DELETE", id, "").await;
    assert_eq!(status, StatusCode::OK, "{stopped}");
    assert_eq!(stopped["state"], "stopped");
    assert!(stopped["host"].is_null());
    assert_eq!(c.fake.running_count(), 0);

    let (status, _) = c.act(&alice, "DELETE", id, "").await;
    assert_eq!(status, StatusCode::CONFLICT, "nothing left to stop");
}

#[tokio::test]
async fn one_instance_at_a_time_and_restart_waits_for_the_cooldown() {
    let c = Ctx::new(roomy()).await;
    let a = c.challenge(json!({ "title": "A" })).await;
    let b = c.challenge(json!({ "title": "B" })).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;

    assert_eq!(c.act(&alice, "POST", a, "").await.0, StatusCode::CREATED);
    assert_eq!(
        c.act(&alice, "POST", a, "").await.0,
        StatusCode::CONFLICT,
        "already running"
    );
    let (status, body) = c.act(&alice, "POST", b, "").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    let (status, body) = c.act(&alice, "POST", a, "/restart").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"].as_str().unwrap().contains("30 seconds"));

    c.w.set_time(T0 + 31);
    let (status, body) = c.act(&alice, "POST", a, "/restart").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["starts_used"], 2);
    assert_eq!(c.fake.running_count(), 1, "the old container is gone");
}

#[tokio::test]
async fn the_start_budget_is_enforced_and_failed_starts_are_free() {
    let policy = Policy {
        restart_cooldown_secs: 0,
        max_starts: 2,
        max_concurrent: 1,
    };
    let c = Ctx::new(policy).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;

    c.fake.set_fail_starts(true);
    let (status, _) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    c.fake.set_fail_starts(false);

    assert_eq!(c.act(&alice, "POST", id, "").await.0, StatusCode::CREATED);
    assert_eq!(
        c.act(&alice, "POST", id, "/restart").await.0,
        StatusCode::OK
    );
    let (status, body) = c.act(&alice, "POST", id, "/restart").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"].as_str().unwrap().contains("limit of 2"));
}

#[tokio::test]
async fn an_expired_instance_reads_expired_refuses_actions_and_is_reaped() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({ "instance_config": spec(60) })).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    assert_eq!(c.act(&alice, "POST", id, "").await.0, StatusCode::CREATED);

    c.w.set_time(T0 + 60);
    let (_, view) = c.act(&alice, "GET", id, "").await;
    assert_eq!(view["state"], "expired");
    assert!(view["host"].is_null());
    assert_eq!(
        c.act(&alice, "POST", id, "/restart").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        c.act(&alice, "DELETE", id, "").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(c.fake.running_count(), 1, "still inside the grace period");

    c.w.set_time(T0 + 65);
    assert_eq!(instances::reap(&c.state).await.unwrap(), 1);
    assert_eq!(c.fake.running_count(), 0);

    let (status, again) = c.act(&alice, "POST", id, "").await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a fresh start is a new run: {again}"
    );
    assert_eq!(again["starts_used"], 2);
}

#[tokio::test]
async fn instances_follow_the_event_clock() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;

    let (status, _) = c.act(&alice, "POST", id, "").await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "registration only: not live yet"
    );

    c.set_state("live").await;
    c.set_state("paused").await;
    assert_eq!(
        c.act(&alice, "POST", id, "").await.0,
        StatusCode::CONFLICT,
        "paused"
    );

    c.set_state("live").await;
    assert_eq!(c.act(&alice, "POST", id, "").await.0, StatusCode::CREATED);

    c.set_state("ended").await;
    assert_eq!(
        instances::reap(&c.state).await.unwrap(),
        1,
        "ending the event stops it"
    );
    assert_eq!(c.fake.running_count(), 0);
}

#[tokio::test]
async fn a_retired_challenge_stops_its_instances_and_refuses_new_ones() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    assert_eq!(c.act(&alice, "POST", id, "").await.0, StatusCode::CREATED);

    let (status, body) = c
        .admin("POST", &format!("/admin/challenges/{id}/retire"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(instances::reap(&c.state).await.unwrap(), 1);
    assert_eq!(c.fake.running_count(), 0);
    assert_eq!(c.act(&alice, "POST", id, "").await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn orphaned_containers_are_removed_but_live_ones_are_kept() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    assert_eq!(c.act(&alice, "POST", id, "").await.0, StatusCode::CREATED);
    c.fake
        .add_orphan("9999", serde_json::from_value(spec(60)).unwrap());
    assert_eq!(c.fake.running_count(), 2);
    assert_eq!(instances::reap(&c.state).await.unwrap(), 1);
    assert_eq!(c.fake.running_count(), 1);
}

#[tokio::test]
async fn instances_belong_to_their_owner_and_only_instance_challenges_have_them() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({})).await;
    let plain = c
        .challenge(json!({ "title": "Plain", "requires_instance": false, "instance_config": null }))
        .await;
    let alice = c.live_player("alice").await;
    let bob = c.live_player("bob").await;
    c.set_state("live").await;
    assert_eq!(c.act(&alice, "POST", id, "").await.0, StatusCode::CREATED);

    let (_, bobs) = c.act(&bob, "GET", id, "").await;
    assert_eq!(bobs["state"], "none", "bob does not see alice's run");
    assert_eq!(c.act(&bob, "DELETE", id, "").await.0, StatusCode::CONFLICT);
    assert_eq!(
        c.act(&alice, "POST", plain, "").await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/events/{}/challenges/{id}/instance", c.slug),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_instance_challenge_needs_a_valid_setup_and_unregistered_players_are_refused() {
    let c = Ctx::new(roomy()).await;
    let base = format!("/admin/events/{}/challenges", c.event_id);
    let mut body = json!({
        "category_id": c.category,
        "title": "Box",
        "description": "x",
        "scoring": { "kind": "static", "points": 100 },
        "flag": { "kind": "static", "value": FLAG },
        "requires_instance": true,
    });
    let (status, _) = c.admin("POST", &base, Some(body.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "no config");

    for bad in [
        json!({ "image": "-v /:/host", "port": 80, "cpu_millis": 100, "memory_mb": 64, "pids": 32, "ttl_secs": 60 }),
        json!({ "image": "ok:1", "port": 80, "cpu_millis": 99999, "memory_mb": 64, "pids": 32, "ttl_secs": 60 }),
        json!({ "image": "ok:1", "port": 80, "cpu_millis": 100, "memory_mb": 64, "pids": 32, "ttl_secs": 999999 }),
    ] {
        body["instance_config"] = bad;
        let (status, reply) = c.admin("POST", &base, Some(body.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{reply}");
    }

    let id = c.challenge(json!({})).await;
    let (stranger, _) = player(&c.w.app, "stranger").await;
    c.set_state("live").await;
    assert_eq!(
        c.act(&stranger, "POST", id, "").await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn admins_list_and_stop_any_run_and_players_cannot() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    let (_, started) = c.act(&alice, "POST", id, "").await;
    let run = started["id"].as_i64().unwrap();

    let (status, list) = c
        .admin(
            "GET",
            &format!("/admin/events/{}/instances", c.event_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["instances"][0]["id"], run);
    assert_eq!(list["instances"][0]["state"], "running");
    assert_eq!(
        list["instances"][0]["username"], "alice",
        "owners are named, not numbered"
    );
    assert!(list["instances"][0]["challenge_title"].is_string());
    assert!(
        !list.to_string().contains("fake-"),
        "handles stay server side"
    );

    let (status, _) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/admin/events/{}/instances", c.event_id),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &c.w.app,
        "DELETE",
        &format!("/api/v1/admin/instances/{run}"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, stopped) = c
        .admin("DELETE", &format!("/admin/instances/{run}"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{stopped}");
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(c.fake.running_count(), 0);
}

#[tokio::test]
async fn a_published_instance_waits_for_its_port_and_hides_the_address_until_then() {
    let c = Ctx::new(roomy()).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;

    c.fake.set_unreachable(true);
    let (status, started) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    assert_eq!(started["state"], "starting");
    assert!(
        started["host"].is_null() && started["port"].is_null(),
        "{started}"
    );

    c.fake.set_unreachable(false);
    let (_, ready) = c.act(&alice, "GET", id, "").await;
    assert_eq!(ready["state"], "running");
    assert_eq!(ready["host"], "play.example.org");
    assert!(ready["port"].as_i64().unwrap() > 0);

    c.fake.set_exited(&c.fake.list_handles()[0]);
    let (_, dead) = c.act(&alice, "GET", id, "").await;
    assert_eq!(
        (dead["state"].clone(), dead["failure"].clone()),
        (json!("failed"), json!("exited"))
    );
    assert!(dead["host"].is_null());
}

// ---------------------------------------------------------------------------------------------
// Lab hosts: the platform runs an event's instances on another machine through its API.
// ---------------------------------------------------------------------------------------------

use async_trait::async_trait;
use kymo_instances::{
    InstanceRuntime, Managed, RuntimeError, RuntimeStatus, StartRequest, Started,
};
use kymo_lab::server::{Lab, LabConfig};

const LAB_TOKEN: &str = "lab-test-token-with-enough-length-1";

/// A runtime that really listens on each instance's port, so the platform's reachability check
/// (a TCP connect to the lab host's public address) has something to connect to.
struct PortRuntime {
    inner: FakeRuntime,
    listeners: std::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>,
    ports: std::sync::Mutex<std::collections::HashMap<String, u16>>,
}

impl PortRuntime {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: FakeRuntime::new(),
            listeners: Default::default(),
            ports: Default::default(),
        })
    }
    fn running(&self) -> usize {
        self.inner.running_count()
    }
}

#[async_trait]
impl InstanceRuntime for PortRuntime {
    async fn start(&self, request: &StartRequest) -> Result<Started, RuntimeError> {
        let started = self.inner.start(request).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move { while listener.accept().await.is_ok() {} });
        self.listeners
            .lock()
            .unwrap()
            .insert(started.handle.clone(), task);
        self.ports
            .lock()
            .unwrap()
            .insert(started.handle.clone(), port);
        Ok(Started {
            handle: started.handle,
            host_port: port,
            ip: None,
        })
    }
    async fn stop(&self, handle: &str) -> Result<(), RuntimeError> {
        if let Some(task) = self.listeners.lock().unwrap().remove(handle) {
            task.abort();
        }
        self.inner.stop(handle).await
    }
    async fn status(&self, handle: &str) -> Result<RuntimeStatus, RuntimeError> {
        self.inner.status(handle).await
    }
    async fn list_managed(&self) -> Result<Vec<Managed>, RuntimeError> {
        self.inner.list_managed().await
    }
    async fn ensure_lab(&self, n: &str, s: &str) -> Result<(), RuntimeError> {
        self.inner.ensure_lab(n, s).await
    }
    async fn remove_lab(&self, n: &str) -> Result<(), RuntimeError> {
        self.inner.remove_lab(n).await
    }
    async fn reachable(&self, port: u16) -> bool {
        self.inner.reachable(port).await
    }
}

struct LabHost {
    url: String,
    runtime: Arc<PortRuntime>,
    client: kymo_lab::RemoteRuntime,
}

/// A real lab host (the same server `kymo-lab` runs) on loopback, with a manifest for the test
/// challenge's image.
async fn lab_host() -> LabHost {
    let manifest: kymo_lab::Manifest = serde_json::from_value(json!({
        "artifact": "registry.example.org/ctf/web-1:1.0",
        "port": 8000,
        "env_allow": ["SECRET_SEED"],
        "max_ttl_secs": 600,
    }))
    .unwrap();
    let mut cfg = LabConfig::new("test-lab", LAB_TOKEN);
    cfg.manifests = vec![manifest];
    let runtime = PortRuntime::new();
    let lab = Lab::new(cfg, runtime.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(kymo_lab::server::serve(
        listener,
        kymo_lab::server::router(lab),
        None,
        std::future::pending(),
    ));
    let client = kymo_lab::RemoteRuntime::new(&url, LAB_TOKEN, None, "127.0.0.1").unwrap();
    LabHost {
        url,
        runtime,
        client,
    }
}

impl Ctx {
    /// Adds a lab host through the admin API and returns its id.
    async fn add_lab(&self, name: &str, url: &str, token: &str) -> (StatusCode, Value) {
        self.admin(
            "POST",
            "/admin/labs",
            Some(
                json!({ "name": name, "api_url": url, "token": token, "public_host": "127.0.0.1" }),
            ),
        )
        .await
    }

    async fn use_lab(&self, lab: Option<i64>) -> (StatusCode, Value) {
        self.admin(
            "PUT",
            &format!("/admin/events/{}/lab", self.event_id),
            Some(json!({ "lab_host_id": lab })),
        )
        .await
    }
}

#[tokio::test]
async fn an_event_on_a_lab_host_runs_its_instances_there_and_players_get_the_labs_address() {
    let lab = lab_host().await;
    let c = Ctx::new(roomy()).await;
    let (status, added) = c.add_lab("Lab A", &lab.url, LAB_TOKEN).await;
    assert_eq!(status, StatusCode::CREATED, "{added}");
    assert_eq!(added["check"]["ok"], true, "{added}");
    assert_eq!(added["check"]["health"]["mode"], "manifest");
    assert!(
        !added.to_string().contains(LAB_TOKEN),
        "the token is never sent back"
    );
    let lab_id = added["lab"]["id"].as_i64().unwrap();

    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    assert_eq!(c.use_lab(Some(lab_id)).await.0, StatusCode::OK);

    let (status, started) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    assert_eq!(started["state"], "running", "{started}");
    assert_eq!(
        started["host"], "127.0.0.1",
        "the lab host's address, not this server's"
    );
    assert_eq!(c.fake.running_count(), 0, "nothing ran on this machine");
    assert_eq!(lab.runtime.running(), 1, "it ran on the lab host");

    // The platform built the name and recorded it; the lab host runs it under that name.
    let row: (String, Option<i64>, String) = sqlx::query_as(
        "SELECT name, lab_host_id, public_host FROM instance ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&c.state.pool)
    .await
    .unwrap();
    assert!(row.0.starts_with(&format!("kymo-c{id}-u")), "{}", row.0);
    assert_eq!((row.1, row.2.as_str()), (Some(lab_id), "127.0.0.1"));
    let listed = lab.client.instances().await.unwrap();
    assert_eq!(listed[0].name, row.0);
    assert_eq!(
        listed[0].meta.get("challenge").map(String::as_str),
        Some(id.to_string().as_str())
    );
    assert!(
        !started.to_string().contains("SECRET_SEED"),
        "setup never leaks"
    );

    // Restart goes to the same lab host as well, and replaces the container there.
    c.set_time_forward(40);
    let (status, again) = c.act(&alice, "POST", id, "/restart").await;
    assert!(status.is_success(), "{again}");
    assert_eq!(again["state"], "running", "{again}");
    assert_eq!(
        lab.runtime.running(),
        1,
        "the old container was replaced, not added to"
    );
    assert_eq!(c.fake.running_count(), 0);

    let (status, stopped) = c.act(&alice, "DELETE", id, "").await;
    assert_eq!(status, StatusCode::OK, "{stopped}");
    assert_eq!(lab.runtime.running(), 0, "stopped on the lab host");
}

impl Ctx {
    fn set_time_forward(&self, secs: i64) {
        self.w.clock.fetch_add(secs, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn lab_hosts_are_for_admins_and_every_field_is_checked() {
    let lab = lab_host().await;
    let c = Ctx::new(roomy()).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    for (method, uri) in [
        ("GET", "/api/v1/admin/labs"),
        ("POST", "/api/v1/admin/labs"),
    ] {
        let (status, _) = call(&c.w.app, method, uri, Some(&alice), Some(json!({}))).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    let bad = |body: Value| async { c.admin("POST", "/admin/labs", Some(body)).await.0 };
    let base = json!({ "name": "Lab", "api_url": lab.url, "token": LAB_TOKEN, "public_host": "127.0.0.1" });
    let with = |k: &str, v: Value| {
        let mut b = base.clone();
        b[k] = v;
        b
    };
    assert_eq!(
        bad(with("api_url", json!("ftp://x"))).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        bad(with("token", json!("short"))).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(bad(with("name", json!("x"))).await, StatusCode::BAD_REQUEST);
    assert_eq!(
        bad(with("public_host", json!("not a host!"))).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        bad(with("fingerprint", json!("zz"))).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        bad(with("fingerprint", json!("AB".repeat(32)))).await,
        StatusCode::BAD_REQUEST,
        "a pin needs https"
    );

    let (status, made) = c.admin("POST", "/admin/labs", Some(base.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(bad(base).await, StatusCode::CONFLICT, "names are unique");
    let id = made["lab"]["id"].as_i64().unwrap();

    // An update with no token keeps the stored one, so the lab host still answers.
    let (status, updated) = c
        .admin(
            "PUT",
            &format!("/admin/labs/{id}"),
            Some(json!({ "name": "Renamed", "public_host": "play.example.org" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["lab"]["name"], "Renamed");
    assert_eq!(updated["lab"]["public_host"], "play.example.org");
    let (_, tested) = c
        .admin("POST", &format!("/admin/labs/{id}/test"), None)
        .await;
    assert_eq!(tested["ok"], true, "{tested}");
    assert!(!updated.to_string().contains(LAB_TOKEN));
    let (_, list) = c.admin("GET", "/admin/labs", None).await;
    assert_eq!(list["labs"][0]["events"], 0);
    assert!(!list.to_string().contains(LAB_TOKEN));
}

#[tokio::test]
async fn a_wrong_token_or_an_unreachable_lab_host_is_reported_and_a_start_fails_cleanly() {
    let lab = lab_host().await;
    let c = Ctx::new(roomy()).await;
    let (_, wrong) = c
        .add_lab("Wrong token", &lab.url, "another-token-another-token-1234")
        .await;
    assert_eq!(wrong["check"]["ok"], false, "{wrong}");
    assert!(
        wrong["check"]["error"]
            .as_str()
            .unwrap()
            .contains("refused"),
        "{wrong}"
    );

    let (_, down) = c.add_lab("Down", "http://127.0.0.1:1", LAB_TOKEN).await;
    assert_eq!(down["check"]["ok"], false);
    let down_id = down["lab"]["id"].as_i64().unwrap();

    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    assert_eq!(c.use_lab(Some(down_id)).await.0, StatusCode::OK);
    let (status, body) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let live: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM instance WHERE state IN ('starting','running')")
            .fetch_one(&c.state.pool)
            .await
            .unwrap();
    assert_eq!(live, 0, "a failed start leaves no live run");

    // Operations sees the lab host as down and the platform as degraded.
    let (_, health) = c.admin("GET", "/admin/ops/health", None).await;
    let labs = health["labs"].as_array().unwrap();
    assert!(
        labs.iter()
            .any(|l| l["name"] == "Down" && l["reachable"] == false),
        "{health}"
    );
    assert_eq!(health["status"], "degraded");
}

#[tokio::test]
async fn the_reaper_removes_orphans_on_a_lab_host_but_keeps_what_the_platform_owns() {
    let lab = lab_host().await;
    let c = Ctx::new(roomy()).await;
    let (_, added) = c.add_lab("Lab A", &lab.url, LAB_TOKEN).await;
    let lab_id = added["lab"]["id"].as_i64().unwrap();
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    c.use_lab(Some(lab_id)).await;
    let (_, started) = c.act(&alice, "POST", id, "").await;
    assert_eq!(started["state"], "running", "{started}");

    // A run the lab host has but the platform does not list, as a crash between the two leaves.
    let orphan = |instance_id: &str, name: &str| kymo_instances::StartRequest {
        instance_id: instance_id.into(),
        spec: InstanceSpecLike::spec(),
        placement: kymo_instances::Placement::Published,
        name: Some(name.into()),
        meta: vec![],
    };
    lab.client
        .start(&orphan("99999", "kymo-c9-u9-aaaaaa"))
        .await
        .unwrap();
    assert_eq!(lab.runtime.running(), 2);
    let ended = instances::reap(&c.state).await.unwrap();
    assert!(ended >= 1, "ended {ended}");
    assert_eq!(
        lab.runtime.running(),
        1,
        "the orphan went, the player's instance stayed"
    );

    // The admin view flags what the platform does not know, and reconcile removes it, along
    // with a container the lab host never recorded.
    lab.client
        .start(&orphan("88888", "kymo-c9-u9-bbbbbb"))
        .await
        .unwrap();
    lab.runtime
        .inner
        .add_orphan("77777", InstanceSpecLike::spec());
    let (_, seen) = c
        .admin("GET", &format!("/admin/labs/{lab_id}/instances"), None)
        .await;
    let unknown = seen["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["known"] == false)
        .count();
    assert_eq!(unknown, 1, "{seen}");
    let (status, done) = c
        .admin("POST", &format!("/admin/labs/{lab_id}/reconcile"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["removed"].as_array().unwrap().len(), 2, "{done}");
    assert_eq!(
        lab.runtime.running(),
        1,
        "only the player's instance is left"
    );
}

/// The spec FakeRuntime wants for an orphan.
struct InstanceSpecLike;
impl InstanceSpecLike {
    fn spec() -> kymo_instances::InstanceSpec {
        serde_json::from_value(spec(600)).unwrap()
    }
}

#[tokio::test]
async fn a_lab_host_in_use_cannot_be_removed_and_a_switched_off_one_takes_no_new_starts() {
    let lab = lab_host().await;
    let c = Ctx::new(roomy()).await;
    let (_, added) = c.add_lab("Lab A", &lab.url, LAB_TOKEN).await;
    let lab_id = added["lab"]["id"].as_i64().unwrap();
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    assert_eq!(
        c.use_lab(Some(9999)).await.0,
        StatusCode::BAD_REQUEST,
        "no such lab host"
    );
    c.use_lab(Some(lab_id)).await;

    // Used by an event: refused. Running instances: the event cannot be moved.
    assert_eq!(
        c.admin("DELETE", &format!("/admin/labs/{lab_id}"), None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (_, started) = c.act(&alice, "POST", id, "").await;
    assert_eq!(started["state"], "running");
    assert_eq!(
        c.use_lab(None).await.0,
        StatusCode::CONFLICT,
        "stop the instances first"
    );

    // Switched off: no new starts, but the running one can still be stopped on the right machine.
    let (status, _) = c
        .admin(
            "PUT",
            &format!("/admin/labs/{lab_id}"),
            Some(json!({ "enabled": false })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, stopped) = c.act(&alice, "DELETE", id, "").await;
    assert_eq!(status, StatusCode::OK, "{stopped}");
    assert_eq!(lab.runtime.running(), 0);
    c.set_time_forward(40);
    let (status, body) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");

    // Move the event back to this machine, then the lab host can go.
    assert_eq!(c.use_lab(None).await.0, StatusCode::OK);
    assert_eq!(
        c.admin("DELETE", &format!("/admin/labs/{lab_id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, list) = c.admin("GET", "/admin/labs", None).await;
    assert!(list["labs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_lab_host_refuses_what_its_manifest_does_not_allow_and_the_player_sees_a_plain_failure() {
    let lab = lab_host().await;
    let c = Ctx::new(roomy()).await;
    let (_, added) = c.add_lab("Lab A", &lab.url, LAB_TOKEN).await;
    let lab_id = added["lab"]["id"].as_i64().unwrap();
    // This challenge asks for an image the lab host has no manifest for.
    let other = c
        .challenge(json!({
            "title": "Other",
            "instance_config": { "image": "registry.example.org/ctf/unlisted:1", "port": 80, "cpu_millis": 500, "memory_mb": 128, "pids": 64, "ttl_secs": 600 },
        }))
        .await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    c.use_lab(Some(lab_id)).await;
    let (status, body) = c.act(&alice, "POST", other, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        !body.to_string().contains("manifest"),
        "lab host details stay off the player's screen: {body}"
    );
    assert_eq!(lab.runtime.running(), 0);
}

#[tokio::test]
async fn a_platform_with_no_local_containers_asks_for_a_lab_host_and_the_vpn_stays_local() {
    let (w, state) = {
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let clock = Arc::new(AtomicI64::new(T0));
        let shared = Arc::clone(&clock);
        let state = AppState::new(pool, PASSWORD)
            .unwrap()
            .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)))
            .with_instances(InstanceSetup {
                runtime: Arc::new(kymo_server::labs::NoLocalRuntime),
                policy: roomy(),
                caps: ResourceCaps::default(),
                public_host: "127.0.0.1".into(),
                reap_grace_secs: 5,
            });
        (
            World {
                app: router(state.clone()),
                clock,
            },
            state,
        )
    };
    let c = Ctx::build(w, state, Arc::new(FakeRuntime::new())).await;
    let id = c.challenge(json!({})).await;
    let alice = c.live_player("alice").await;
    c.set_state("live").await;
    let (status, body) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body["error"].as_str().unwrap().contains("lab host"),
        "{body}"
    );

    // With a lab host chosen it works, and the VPN cannot be combined with one.
    let lab = lab_host().await;
    let (_, added) = c.add_lab("Lab A", &lab.url, LAB_TOKEN).await;
    c.use_lab(added["lab"]["id"].as_i64()).await;
    let (status, started) = c.act(&alice, "POST", id, "").await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    assert_eq!(started["state"], "running");
    let (status, refused) = c
        .admin(
            "PUT",
            &format!("/admin/events/{}/network", c.event_id),
            Some(json!({ "mode": "vpn" })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("lab host"),
        "{refused}"
    );
}
