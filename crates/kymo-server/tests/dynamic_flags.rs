use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;
const FLAG: &str = "KYMO{one}";

struct World {
    app: Router,
}

async fn world() -> (World, AppState) {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let clock = Arc::new(AtomicI64::new(T0));
    let shared = Arc::clone(&clock);
    let state = AppState::new(pool, PASSWORD)
        .unwrap()
        .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
    (
        World {
            app: router(state.clone()),
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
    admin: String,
    event_id: i64,
    slug: String,
    category: i64,
}

impl Ctx {
    async fn new() -> Ctx {
        let (w, state) = world().await;
        Ctx::build(w, state).await
    }

    async fn build(w: World, state: AppState) -> Ctx {
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
}

fn dynamic_body(extra: Value) -> Value {
    let mut body = json!({
        "flag_prefix": "KYMO{",
        "flag_suffix": "}",
        "flag_mode": "dynamic",
        "flags": [ { "name": "flag", "value": "KYMO{box_", "closing": "}" } ],
    });
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    body
}

impl Ctx {
    /// What a start does on a lab host, without one: the run row and its generated flags.
    async fn seed_run(
        &self,
        user: i64,
        challenge: i64,
        state: &str,
        flags: &[(&str, &str)],
    ) -> i64 {
        let run: i64 = sqlx::query_scalar(
            "INSERT INTO instance (event_id, challenge_id, scorer_kind, scorer_id, user_id, state, started_at, expires_at) VALUES (?, ?, 'user', ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(self.event_id)
        .bind(challenge)
        .bind(user)
        .bind(user)
        .bind(state)
        .bind(T0)
        .bind(T0 + 3600)
        .fetch_one(&self.state.pool)
        .await
        .unwrap();
        for (name, value) in flags {
            sqlx::query(
                "INSERT INTO instance_flag (instance_id, flag_id, value) SELECT ?, id, ? FROM challenge_flag WHERE challenge_id = ? AND name = ?",
            )
            .bind(run)
            .bind(value)
            .bind(challenge)
            .bind(name)
            .execute(&self.state.pool)
            .await
            .unwrap();
        }
        run
    }

    async fn submit(&self, token: &str, challenge: i64, flag: &str) -> (StatusCode, Value) {
        call(
            &self.w.app,
            "POST",
            &format!("/api/v1/events/{}/challenges/{challenge}/submit", self.slug),
            Some(token),
            Some(json!({ "flag": flag })),
        )
        .await
    }

    async fn listed(&self, token: &str, challenge: i64) -> Value {
        let (status, list) = call(
            &self.w.app,
            "GET",
            &format!("/api/v1/events/{}/challenges", self.slug),
            Some(token),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        list["challenges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == challenge)
            .cloned()
            .unwrap()
    }

    async fn points_of(&self, name: &str) -> i64 {
        let (status, board) = call(
            &self.w.app,
            "GET",
            &format!("/api/v1/events/{}/scoreboard", self.slug),
            Some(&self.admin),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{board}");
        board["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .map(|r| r["points"].as_i64().unwrap())
            .unwrap_or(-1)
    }

    async fn audit_actions(&self) -> Vec<String> {
        let (status, page) = self.admin("GET", "/admin/monitor/audit", None).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["action"].as_str().unwrap().to_owned())
            .collect()
    }
}

async fn create(c: &Ctx, body: Value) -> (StatusCode, Value) {
    let mut full = json!({
        "category_id": c.category,
        "title": "Box",
        "scoring": { "kind": "static", "points": 100 },
        "flag": { "kind": "static", "value": FLAG },
        "requires_instance": true,
        "instance_config": spec(600),
    });
    for (k, v) in body.as_object().unwrap() {
        full[k] = v.clone();
    }
    c.admin(
        "POST",
        &format!("/admin/events/{}/challenges", c.event_id),
        Some(full),
    )
    .await
}

#[tokio::test]
async fn a_plain_challenge_is_untouched_by_the_new_settings() {
    let c = Ctx::new().await;
    let (status, made) = create(&c, json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["flag_mode"], "static");
    assert_eq!(made["multi_flag"], false);
    assert_eq!(made["responder"], false);
    assert_eq!(made["flags"], json!([]));
    assert_eq!(made["flag"]["value"], FLAG);
}

#[tokio::test]
async fn dynamic_and_responder_settings_are_validated() {
    let c = Ctx::new().await;
    // Dynamic needs an instance.
    let (status, body) = create(
        &c,
        dynamic_body(json!({ "requires_instance": false, "instance_config": null })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // The prefix must sit inside the accepted prefix, the closing inside the accepted suffix.
    let (status, _) = create(
        &c,
        dynamic_body(json!({ "flags": [ { "name": "flag", "value": "OTHER{", "closing": "}" } ] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = create(
        &c,
        dynamic_body(
            json!({ "flags": [ { "name": "flag", "value": "KYMO{x_", "closing": ")" } ] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // The random part is 4 to 32 characters.
    for len in [3, 33] {
        let (status, _) = create(
            &c,
            dynamic_body(json!({ "flags": [ { "name": "flag", "value": "KYMO{x_", "closing": "}", "random_len": len } ] })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{len}");
    }
    // Names are lowercase words and unique.
    let (status, _) = create(
        &c,
        dynamic_body(
            json!({ "flags": [ { "name": "Bad Name", "value": "KYMO{x_", "closing": "}" } ] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = create(
        &c,
        dynamic_body(json!({ "flags": [
            { "name": "user", "value": "KYMO{u_", "closing": "}" },
            { "name": "user", "value": "KYMO{r_", "closing": "}" } ] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Several flags need static points, and 1 to 8 flags.
    let two = json!([
        { "name": "user", "value": "KYMO{u_", "closing": "}" },
        { "name": "root", "value": "KYMO{r_", "closing": "}" } ]);
    let (status, _) = create(
        &c,
        dynamic_body(json!({
            "flags": two,
            "scoring": { "kind": "dynamic", "initial": 500, "minimum": 100, "decay": 20, "curve": "parabolic" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let nine: Vec<Value> = (0..9)
        .map(|i| json!({ "name": format!("f{i}"), "value": "KYMO{x_", "closing": "}" }))
        .collect();
    let (status, _) = create(&c, dynamic_body(json!({ "flags": nine }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // A Responder needs an instance and exactly one flag.
    let (status, _) = create(
        &c,
        json!({ "responder": true, "requires_instance": false, "instance_config": null }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = create(&c, dynamic_body(json!({ "responder": true, "flags": two }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // The good shapes work.
    let (status, made) = create(&c, dynamic_body(json!({}))).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["flag_mode"], "dynamic");
    assert_eq!(made["multi_flag"], false);
    assert_eq!(made["flags"][0]["random_len"], 8);
    let (status, made) = create(&c, dynamic_body(json!({ "responder": true }))).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["responder"], true);
    let (status, made) = create(&c, json!({ "responder": true })).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    let (status, made) = create(&c, dynamic_body(json!({ "flags": two }))).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["multi_flag"], true);
}

#[tokio::test]
async fn a_dynamic_flag_is_valid_for_its_owner_only_and_sharing_leaves_a_note() {
    let c = Ctx::new().await;
    let (_, made) = create(&c, dynamic_body(json!({}))).await;
    let id = made["id"].as_i64().unwrap();
    c.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    let (bob, bob_id) = player(&c.w.app, "bob").await;
    c.register(&alice).await;
    c.register(&bob).await;
    c.set_state("live").await;
    c.seed_run(alice_id, id, "running", &[("flag", "KYMO{box_aaaa1111}")])
        .await;
    c.seed_run(bob_id, id, "running", &[("flag", "KYMO{box_bbbb2222}")])
        .await;

    // A flag that does not match the accepted shape is not even an attempt.
    let (_, shape) = c.submit(&alice, id, "hello").await;
    assert_eq!(shape["verdict"], "bad_format");
    // Bob tries Alice's flag: wrong for him, and the organizers are told.
    let (status, wrong) = c.submit(&bob, id, "KYMO{box_aaaa1111}").await;
    assert_eq!(status, StatusCode::OK, "{wrong}");
    assert_eq!(wrong["verdict"], "incorrect");
    assert!(c.audit_actions().await.contains(&"flag_shared".to_owned()));
    // The static flag the challenge was created with means nothing.
    let (_, fixed) = c.submit(&bob, id, FLAG).await;
    assert_eq!(fixed["verdict"], "incorrect");
    // Bob's own flag solves it, for the full points.
    let (_, ok) = c.submit(&bob, id, "KYMO{box_bbbb2222}").await;
    assert_eq!(ok["verdict"], "correct", "{ok}");
    assert_eq!(ok["points"], 100);
    assert_eq!(c.points_of("bob").await, 100);
    // Nobody can solve it twice.
    let (status, _) = c.submit(&bob, id, "KYMO{box_bbbb2222}").await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Alice is unaffected and solves with hers.
    let (_, ok) = c.submit(&alice, id, "KYMO{box_aaaa1111}").await;
    assert_eq!(ok["verdict"], "correct");
}

#[tokio::test]
async fn submitting_another_players_flag_warns_then_bans() {
    let c = Ctx::new().await;
    let (_, made) = create(&c, dynamic_body(json!({}))).await;
    let id = made["id"].as_i64().unwrap();
    c.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    let (bob, bob_id) = player(&c.w.app, "bob").await;
    c.register(&alice).await;
    c.register(&bob).await;
    c.set_state("live").await;
    c.seed_run(alice_id, id, "running", &[("flag", "KYMO{box_aaaa1111}")])
        .await;
    c.seed_run(bob_id, id, "running", &[("flag", "KYMO{box_bbbb2222}")])
        .await;

    // Default rule: one warning, then a ban.
    let (_, first) = c.submit(&bob, id, "KYMO{box_aaaa1111}").await;
    assert_eq!(first["sharing"]["status"], "warning", "{first}");
    assert_eq!(first["sharing"]["strikes"], 1);
    // His own wrong guesses are not strikes.
    let (_, own) = c.submit(&bob, id, "KYMO{box_00000000}").await;
    assert!(own.get("sharing").is_none(), "{own}");
    let (_, second) = c.submit(&bob, id, "KYMO{box_aaaa1111}").await;
    assert_eq!(second["sharing"]["status"], "banned", "{second}");
    assert!(c.audit_actions().await.contains(&"auto_ban".to_owned()));
    let (status, _) = c.submit(&bob, id, "KYMO{box_bbbb2222}").await;
    assert_ne!(status, StatusCode::OK, "a banned player is locked out");
}

#[tokio::test]
async fn the_sharing_rule_can_be_off_or_manual() {
    let c = Ctx::new().await;
    let (_, made) = create(&c, dynamic_body(json!({}))).await;
    let id = made["id"].as_i64().unwrap();
    c.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    let (bob, bob_id) = player(&c.w.app, "bob").await;
    c.register(&alice).await;
    c.register(&bob).await;
    c.set_state("live").await;
    c.seed_run(alice_id, id, "running", &[("flag", "KYMO{box_aaaa1111}")])
        .await;
    c.seed_run(bob_id, id, "running", &[("flag", "KYMO{box_bbbb2222}")])
        .await;
    let path = format!("/admin/events/{}/jeopardy", c.event_id);
    let (status, mut settings) = c.admin("GET", &path, None).await;
    assert_eq!(status, StatusCode::OK, "{settings}");
    settings["sharing_action"] = json!("manual");
    settings["sharing_warnings"] = json!(0);
    let (status, body) = c.admin("PUT", &path, Some(settings.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, one) = c.submit(&bob, id, "KYMO{box_aaaa1111}").await;
    assert_eq!(one["sharing"]["status"], "reported", "{one}");
    assert!(
        c.audit_actions()
            .await
            .contains(&"flag_sharing_review".to_owned())
    );
    // Still allowed to play.
    let (_, ok) = c.submit(&bob, id, "KYMO{box_bbbb2222}").await;
    assert_eq!(ok["verdict"], "correct", "{ok}");

    settings["sharing_action"] = json!("off");
    let (status, _) = c.admin("PUT", &path, Some(settings.clone())).await;
    assert_eq!(status, StatusCode::OK);
    let (_, off) = c.submit(&alice, id, "KYMO{box_bbbb2222}").await;
    assert!(off.get("sharing").is_none(), "{off}");
    settings["sharing_warnings"] = json!(11);
    let (status, _) = c.admin("PUT", &path, Some(settings)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn flags_from_earlier_runs_stay_valid_and_runs_that_never_started_do_not_count() {
    let c = Ctx::new().await;
    let (_, made) = create(&c, dynamic_body(json!({}))).await;
    let id = made["id"].as_i64().unwrap();
    c.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    c.seed_run(alice_id, id, "stopped", &[("flag", "KYMO{box_00000001}")])
        .await;
    let failed = c
        .seed_run(alice_id, id, "failed", &[("flag", "KYMO{box_00000002}")])
        .await;
    sqlx::query("UPDATE instance SET failure = 'start_failed' WHERE id = ?")
        .bind(failed)
        .execute(&c.state.pool)
        .await
        .unwrap();
    c.seed_run(alice_id, id, "running", &[("flag", "KYMO{box_00000003}")])
        .await;
    // A start that failed never gave her the flag.
    let (_, no) = c.submit(&alice, id, "KYMO{box_00000002}").await;
    assert_eq!(no["verdict"], "incorrect");
    // The flag from the run before the reset is still good.
    let (_, ok) = c.submit(&alice, id, "KYMO{box_00000001}").await;
    assert_eq!(ok["verdict"], "correct", "{ok}");
}

async fn boot2root(c: &Ctx, extra: Value) -> i64 {
    let mut body = dynamic_body(json!({
        "flags": [
            { "name": "user", "label": "User flag", "value": "KYMO{user_", "closing": "}", "weight": 1 },
            { "name": "root", "label": "Root flag", "value": "KYMO{root_", "closing": "}", "weight": 3 } ],
    }));
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let (status, made) = create(c, body).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    let id = made["id"].as_i64().unwrap();
    c.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    id
}

#[tokio::test]
async fn several_flags_are_found_one_by_one_with_points_split_by_weight() {
    let c = Ctx::new().await;
    let id = boot2root(&c, json!({})).await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    c.seed_run(
        alice_id,
        id,
        "running",
        &[("user", "KYMO{user_aaaa}"), ("root", "KYMO{root_bbbb}")],
    )
    .await;

    let before = c.listed(&alice, id).await;
    assert_eq!(before["flags_total"], 2);
    assert_eq!(before["flags_found"], 0);
    assert_eq!(before["solved"], false);
    // Names and labels are never shown to players.
    assert!(!before.to_string().contains("Root flag"));

    let (_, first) = c.submit(&alice, id, "KYMO{user_aaaa}").await;
    assert_eq!(first["verdict"], "correct", "{first}");
    assert_eq!(first["points"], 25);
    assert_eq!(first["flags_found"], 1);
    assert_eq!(first["flags_total"], 2);
    assert_eq!(c.points_of("alice").await, 25);
    assert_eq!(c.listed(&alice, id).await["solved"], false);

    // The same flag again scores nothing and is not a wrong try.
    let (status, again) = c.submit(&alice, id, "KYMO{user_aaaa}").await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");

    let (_, second) = c.submit(&alice, id, "KYMO{root_bbbb}").await;
    assert_eq!(second["verdict"], "correct", "{second}");
    assert_eq!(second["points"], 75);
    assert_eq!(second["flags_found"], 2);
    assert_eq!(c.points_of("alice").await, 100);
    let after = c.listed(&alice, id).await;
    assert_eq!(after["solved"], true);
    assert_eq!(after["flags_found"], 2);
    assert_eq!(after["solved_by"], 1);
    // Nothing left to find.
    let (status, _) = c.submit(&alice, id, "KYMO{root_bbbb}").await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_team_shares_its_flags_and_a_wrong_flag_does_not_cost_found_ones() {
    let c = Ctx::new().await;
    let id = boot2root(&c, json!({ "max_attempts": 2 })).await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    c.seed_run(
        alice_id,
        id,
        "running",
        &[("user", "KYMO{user_aaaa}"), ("root", "KYMO{root_bbbb}")],
    )
    .await;
    let (_, ok) = c.submit(&alice, id, "KYMO{user_aaaa}").await;
    assert_eq!(ok["verdict"], "correct");
    // The cap counts wrong tries only, so a found flag did not use one up.
    let (_, w1) = c.submit(&alice, id, "KYMO{root_0000}").await;
    assert_eq!(w1["verdict"], "incorrect");
    assert_eq!(w1["attempts_remaining"], 1);
    let (_, w2) = c.submit(&alice, id, "KYMO{root_1111}").await;
    assert_eq!(w2["attempts_remaining"], 0);
    let (status, _) = c.submit(&alice, id, "KYMO{root_bbbb}").await;
    assert_eq!(status, StatusCode::CONFLICT, "the cap is reached");
}

#[tokio::test]
async fn static_flags_work_in_a_multi_flag_challenge_without_an_instance() {
    let c = Ctx::new().await;
    let (status, made) = create(
        &c,
        json!({
            "requires_instance": false,
            "instance_config": null,
            "flag_prefix": "KYMO{",
            "flag_suffix": "}",
            "flags": [
                { "name": "one", "value": "KYMO{one}" },
                { "name": "two", "value": "KYMO{two}" } ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    let id = made["id"].as_i64().unwrap();
    c.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    let (_, a) = c.submit(&alice, id, "KYMO{two}").await;
    assert_eq!(a["verdict"], "correct", "{a}");
    assert_eq!(a["points"], 50);
    let (_, b) = c.submit(&alice, id, "KYMO{one}").await;
    assert_eq!(b["flags_found"], 2);
    assert_eq!(c.points_of("alice").await, 100);
}

#[tokio::test]
async fn hint_costs_are_taken_from_the_awards_in_the_order_flags_are_found() {
    let c = Ctx::new().await;
    let id = boot2root(&c, json!({})).await;
    let (status, hint) = c
        .admin(
            "POST",
            &format!("/admin/challenges/{id}/hints"),
            Some(json!({ "content": "look around", "cost": 40 })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{hint}");
    let hint_id = hint["id"].as_i64().unwrap();
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    c.seed_run(
        alice_id,
        id,
        "running",
        &[("user", "KYMO{user_aaaa}"), ("root", "KYMO{root_bbbb}")],
    )
    .await;
    let (status, body) = call(
        &c.w.app,
        "POST",
        &format!(
            "/api/v1/events/{}/challenges/{id}/hints/{hint_id}/unlock",
            c.slug
        ),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // 40 owed: the 25 point flag pays 25 of it, the 75 point flag pays the last 15.
    let (_, first) = c.submit(&alice, id, "KYMO{user_aaaa}").await;
    assert_eq!(first["points"], 0);
    let (_, second) = c.submit(&alice, id, "KYMO{root_bbbb}").await;
    assert_eq!(second["points"], 60);
    assert_eq!(c.points_of("alice").await, 60);
}

#[tokio::test]
async fn the_flag_settings_are_locked_once_a_run_exists_but_labels_can_change() {
    let c = Ctx::new().await;
    let id = boot2root(&c, json!({})).await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    // Before any run the set can be rewritten.
    let (status, body) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}"),
            Some(json!({ "flags": [
                { "name": "user", "value": "KYMO{u_", "closing": "}", "weight": 2 },
                { "name": "root", "value": "KYMO{r_", "closing": "}", "weight": 2 } ] })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    c.seed_run(alice_id, id, "running", &[("user", "KYMO{u_aaaa}")])
        .await;
    let flags = |weight: i64, label: &str| {
        json!({ "flags": [
            { "name": "user", "label": label, "value": "KYMO{u_", "closing": "}", "weight": weight },
            { "name": "root", "value": "KYMO{r_", "closing": "}", "weight": 2 } ] })
    };
    let (status, _) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}"),
            Some(flags(5, "")),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, body) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}"),
            Some(flags(2, "Renamed")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["flags"][0]["label"], "Renamed");
    // Switching a dynamic challenge back to a plain static one after a run is refused too.
    let (status, _) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}"),
            Some(json!({ "flag_mode": "static", "flags": [], "flag": { "kind": "static", "value": "KYMO{x}" } })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_bundle_carries_the_flag_settings_and_a_plain_challenge_exports_as_before() {
    let c = Ctx::new().await;
    boot2root(&c, json!({ "responder": false })).await;
    let (_, plain) = create(&c, json!({ "title": "Plain" })).await;
    assert!(plain["id"].is_i64());
    let (status, bundle) = c
        .admin("GET", &format!("/admin/events/{}/export", c.event_id), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{bundle}");
    let parts = bundle["challenges"].as_array().unwrap();
    let multi = parts.iter().find(|p| p["title"] == "Box").unwrap();
    assert_eq!(multi["flag_mode"], "dynamic");
    assert_eq!(multi["flags"].as_array().unwrap().len(), 2);
    let plain = parts.iter().find(|p| p["title"] == "Plain").unwrap();
    for key in ["flag_mode", "responder", "flags"] {
        assert!(plain.get(key).is_none(), "{key} is written only when used");
    }
    // Importing it back keeps the settings; an older bundle without the keys still imports.
    let (status, imported) = c
        .admin("POST", "/admin/events/import", Some(bundle.clone()))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{imported}");
    let new_id = imported["id"].as_i64().unwrap();
    let (_, list) = c
        .admin("GET", &format!("/admin/events/{new_id}/challenges"), None)
        .await;
    let box_challenge = list
        .as_array()
        .unwrap()
        .iter()
        .find(|ch| ch["title"] == "Box")
        .unwrap();
    assert_eq!(box_challenge["flag_mode"], "dynamic");
    assert_eq!(box_challenge["multi_flag"], true);
    assert_eq!(box_challenge["flags"][1]["weight"], 3);
}

#[tokio::test]
async fn an_organizer_can_read_a_runs_flags_and_the_reading_is_audited() {
    let c = Ctx::new().await;
    let id = boot2root(&c, json!({})).await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    let run = c
        .seed_run(
            alice_id,
            id,
            "running",
            &[("user", "KYMO{user_aaaa}"), ("root", "KYMO{root_bbbb}")],
        )
        .await;
    // Players cannot.
    let (status, _) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/admin/instances/{run}/flags"),
        Some(&alice),
        None,
    )
    .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
        "{status}"
    );
    let (status, body) = c
        .admin("GET", &format!("/admin/instances/{run}/flags"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let flags = body["flags"].as_array().unwrap();
    assert_eq!(flags.len(), 2);
    assert_eq!(flags[0]["name"], "user");
    assert_eq!(flags[0]["value"], "KYMO{user_aaaa}");
    assert_eq!(flags[1]["label"], "Root flag");
    assert!(
        c.audit_actions()
            .await
            .contains(&"instance_flags_viewed".to_owned())
    );
    // An instance that does not exist is a plain 404.
    let (status, _) = c.admin("GET", "/admin/instances/99999/flags", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
