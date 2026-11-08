use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;
const HOUR: i64 = 3600;
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

async fn world() -> World {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let clock = Arc::new(AtomicI64::new(T0));
    let shared = Arc::clone(&clock);
    let state = AppState::new(pool, PASSWORD)
        .unwrap()
        .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
    World {
        app: router(state),
        clock,
    }
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
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
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Reply {
        status,
        headers,
        bytes,
    }
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

struct Ctx {
    w: World,
    admin: String,
    event_id: i64,
    slug: String,
    category: i64,
}

impl Ctx {
    async fn new(mode: &str, extra: Value) -> Ctx {
        let w = world().await;
        let admin = admin_token(&w.app).await;
        let mut body = json!({ "name": "Spring CTF", "kind": "jeopardy", "mode": mode });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (status, event) = call(
            &w.app,
            "POST",
            "/api/v1/admin/events",
            Some(&admin),
            Some(body),
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
            admin,
            event_id,
            slug: event["slug"].as_str().unwrap().to_owned(),
            category: category["id"].as_i64().unwrap(),
        };
        ctx.set_state("registration_open").await;
        ctx
    }

    async fn set_state(&self, state: &str) {
        let (status, body) = call(
            &self.w.app,
            "POST",
            &format!("/api/v1/admin/events/{}/state", self.event_id),
            Some(&self.admin),
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

    /// Creates and publishes a challenge. `extra` overrides the default body.
    async fn challenge(&self, extra: Value) -> i64 {
        let id = self.draft(extra).await;
        let (status, body) = self
            .admin("POST", &format!("/admin/challenges/{id}/publish"), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        id
    }

    async fn draft(&self, extra: Value) -> i64 {
        let mut body = json!({
            "category_id": self.category,
            "title": "Warmup",
            "description": "find it",
            "scoring": { "kind": "static", "points": 100 },
            "flag": { "kind": "static", "value": FLAG },
            "flag_prefix": "KYMO{",
            "flag_suffix": "}",
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
        created["id"].as_i64().unwrap()
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

    async fn list(&self, token: &str) -> (StatusCode, Value) {
        call(
            &self.w.app,
            "GET",
            &format!("/api/v1/events/{}/challenges", self.slug),
            Some(token),
            None,
        )
        .await
    }

    async fn board(&self, token: Option<&str>) -> (StatusCode, Value) {
        call(
            &self.w.app,
            "GET",
            &format!("/api/v1/events/{}/scoreboard?matrix=true", self.slug),
            token,
            None,
        )
        .await
    }

    async fn history(&self, query: &str) -> Value {
        let (status, body) = self
            .admin(
                "GET",
                &format!("/admin/events/{}/submissions?{query}", self.event_id),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }
}

fn challenge_in(list: &Value, id: i64) -> Option<&Value> {
    list["challenges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
}

#[tokio::test]
async fn challenge_crud_and_validation() {
    let c = Ctx::new("solo", json!({})).await;
    let base = format!("/admin/events/{}/challenges", c.event_id);

    let (status, dup) = c
        .admin(
            "POST",
            &format!("/admin/events/{}/categories", c.event_id),
            Some(json!({ "name": "Web" })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{dup}");

    let id = c.draft(json!({ "max_attempts": 5 })).await;
    let (status, shown) = c
        .admin("GET", &format!("/admin/challenges/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shown["flag"]["value"], FLAG, "admins may see the flag");
    assert_eq!(shown["state"], "draft");
    assert_eq!(shown["max_attempts"], 5);

    let good = json!({
        "category_id": c.category, "title": "T",
        "scoring": { "kind": "static", "points": 10 },
        "flag": { "kind": "static", "value": "x" },
    });
    let mutate = |k: &str, v: Value| {
        let mut b = good.clone();
        b[k] = v;
        b
    };
    for (name, body) in [
        (
            "invalid regex",
            mutate("flag", json!({ "kind": "regex", "pattern": "(unclosed" })),
        ),
        (
            "empty flag",
            mutate("flag", json!({ "kind": "static", "value": " " })),
        ),
        (
            "zero decay",
            mutate(
                "scoring",
                json!({ "kind": "dynamic", "initial": 500, "minimum": 100, "decay": 0, "curve": "linear" }),
            ),
        ),
        (
            "minimum above initial",
            mutate(
                "scoring",
                json!({ "kind": "dynamic", "initial": 100, "minimum": 500, "decay": 5, "curve": "linear" }),
            ),
        ),
        (
            "negative points",
            mutate("scoring", json!({ "kind": "static", "points": -1 })),
        ),
        ("release after retire", {
            let mut b = good.clone();
            b["release_at"] = json!(T0 + 100);
            b["retire_at"] = json!(T0 + 50);
            b
        }),
        ("foreign category", mutate("category_id", json!(9999))),
        ("empty title", mutate("title", json!("  "))),
        ("zero max attempts", mutate("max_attempts", json!(0))),
    ] {
        let (status, body) = c.admin("POST", &base, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{name}: {body}");
        assert!(body["error"].is_string());
    }
    let (status, _) = c
        .admin(
            "POST",
            &base,
            Some(mutate(
                "flag",
                json!({ "kind": "regex", "pattern": "KYMO\\{[a-z]+\\}" }),
            )),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a valid regex flag is accepted"
    );

    // Partial update: change the title, clear the attempt cap.
    let (status, updated) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}"),
            Some(json!({ "title": "Renamed", "max_attempts": null })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["title"], "Renamed");
    assert_eq!(updated["max_attempts"], Value::Null);
    assert_eq!(updated["flag"]["value"], FLAG, "untouched fields stay");
    let (status, _) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}"),
            Some(json!({ "flag": { "kind": "regex", "pattern": "[" } })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Hints.
    let (status, _) = c
        .admin(
            "POST",
            &format!("/admin/challenges/{id}/hints"),
            Some(json!({ "content": "x", "cost": -5 })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, hint) = c
        .admin(
            "POST",
            &format!("/admin/challenges/{id}/hints"),
            Some(json!({ "content": "look at the cookie", "cost": 10 })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let hint_id = hint["id"].as_i64().unwrap();
    let (status, hint) = c
        .admin(
            "PUT",
            &format!("/admin/hints/{hint_id}"),
            Some(json!({ "cost": 20 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hint["cost"], 20);

    // Schedule, publish, retire.
    let (status, scheduled) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}/schedule"),
            Some(json!({ "release_at": T0 + 10, "retire_at": T0 + 20 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    assert_eq!(scheduled["release_at"], T0 + 10);
    let (status, _) = c
        .admin(
            "PUT",
            &format!("/admin/challenges/{id}/schedule"),
            Some(json!({ "release_at": T0 + 30, "retire_at": T0 + 20 })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, published) = c
        .admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    assert_eq!(published["state"], "published");
    let (_, retired) = c
        .admin("POST", &format!("/admin/challenges/{id}/retire"), None)
        .await;
    assert_eq!(retired["state"], "retired");

    // Listing carries hints; category with challenges cannot be deleted; players are refused.
    let (_, listed) = c.admin("GET", &base, None).await;
    assert_eq!(listed.as_array().unwrap().len(), 2);
    assert_eq!(listed[0]["hints"].as_array().unwrap().len(), 1);
    let (status, _) = c
        .admin("DELETE", &format!("/admin/categories/{}", c.category), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (alice, _) = player(&c.w.app, "alice").await;
    let (status, _) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/admin/challenges/{id}"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/admin/challenges/{id}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = c
        .admin("DELETE", &format!("/admin/hints/{hint_id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = c
        .admin("DELETE", &format!("/admin/challenges/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = c
        .admin("GET", &format!("/admin/challenges/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A tournament event has no challenges.
    let (_, tournament_event) = c
        .admin(
            "POST",
            "/admin/events",
            Some(json!({ "name": "Cup", "kind": "tournament", "mode": "team" })),
        )
        .await;
    let (status, _) = c
        .admin(
            "POST",
            &format!("/admin/events/{}/categories", tournament_event["id"]),
            Some(json!({ "name": "X" })),
        )
        .await;
    // Tournament events keep challenges for matches decided by challenges, so they take categories.
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn the_flag_never_reaches_players() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let draft = c.draft(json!({ "title": "Secret draft" })).await;
    let (_, hint) = c
        .admin(
            "POST",
            &format!("/admin/challenges/{id}/hints"),
            Some(json!({ "content": "HINTTEXT", "cost": 10 })),
        )
        .await;
    let hint_id = hint["id"].as_i64().unwrap();
    let (alice, _) = player(&c.w.app, "alice").await;
    let (eve, _) = player(&c.w.app, "eve").await;
    c.register(&alice).await;
    c.set_state("live").await;

    let (status, list) = c.list(&eve).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "not registered");
    let (status, _) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/events/{}/challenges", c.slug),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, list_body) = c.list(&alice).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let text = list_body.to_string();
    assert!(!text.contains(FLAG), "flag leaked: {text}");
    assert!(!text.contains("\"flag\":"), "flag field leaked: {text}");
    assert!(!text.contains("HINTTEXT"), "locked hint content leaked");
    assert!(
        challenge_in(&list_body, draft).is_none(),
        "drafts stay hidden"
    );
    let shown = challenge_in(&list_body, id).unwrap();
    assert_eq!(shown["points"], 100);
    assert_eq!(shown["solved"], false);
    assert_eq!(shown["hints"][0]["unlocked"], false);
    assert_eq!(shown["hints"][0]["cost"], 10);
    assert_eq!(shown["flag_format"]["prefix"], "KYMO{");

    let (status, unlocked) = call(
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
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unlocked["content"], "HINTTEXT");
    let (_, after) = c.list(&alice).await;
    assert_eq!(
        challenge_in(&after, id).unwrap()["hints"][0]["content"],
        "HINTTEXT"
    );
    let (_, board) = c.board(Some(&alice)).await;
    assert!(!board.to_string().contains(FLAG));
    let (_, reply) = c.submit(&alice, id, "KYMO{nope}").await;
    assert!(!reply.to_string().contains(FLAG));
    // A draft challenge cannot be probed by id either.
    let (status, _) = c.submit(&alice, draft, FLAG).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn event_and_challenge_windows_gate_play() {
    let c = Ctx::new(
        "solo",
        json!({ "starts_at": T0 + HOUR, "ends_at": T0 + 10 * HOUR }),
    )
    .await;
    let open = c.challenge(json!({ "title": "Always" })).await;
    let later = c
        .challenge(json!({ "title": "Later", "release_at": T0 + 2 * HOUR }))
        .await;
    let brief = c
        .challenge(json!({ "title": "Brief", "retire_at": T0 + 3 * HOUR }))
        .await;
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;

    // Before the event is live.
    let (status, body) = c.list(&alice).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = c.submit(&alice, open, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"].as_str().unwrap().contains("not started"),
        "{body}"
    );

    // Live: the unreleased challenge is hidden and refuses submissions.
    c.w.set_time(T0 + HOUR + 5);
    let (status, list) = c.list(&alice).await;
    assert_eq!(status, StatusCode::OK);
    assert!(challenge_in(&list, later).is_none());
    assert!(challenge_in(&list, brief).is_some());
    assert_eq!(challenge_in(&list, open).unwrap()["status"], "open");
    let (status, body) = c.submit(&alice, later, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"].as_str().unwrap().contains("not released"),
        "{body}"
    );

    c.w.set_time(T0 + 2 * HOUR);
    let (_, list) = c.list(&alice).await;
    assert!(challenge_in(&list, later).is_some(), "released at its time");

    // After the retire time the challenge is shown closed and refuses submissions.
    c.w.set_time(T0 + 3 * HOUR);
    let (_, list) = c.list(&alice).await;
    assert_eq!(challenge_in(&list, brief).unwrap()["status"], "closed");
    let (status, body) = c.submit(&alice, brief, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"].as_str().unwrap().contains("retired"),
        "{body}"
    );
    let (status, _) = c.submit(&alice, open, FLAG).await;
    assert_eq!(status, StatusCode::OK);

    // Ended by schedule: challenges stay visible, submissions stop.
    c.w.set_time(T0 + 10 * HOUR);
    let (status, list) = c.list(&alice).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(challenge_in(&list, later).unwrap()["status"], "closed");
    let (status, body) = c.submit(&alice, later, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("ended"), "{body}");
}

#[tokio::test]
async fn paused_and_ended_events_refuse_submissions() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    c.set_state("paused").await;
    let (status, body) = c.submit(&alice, id, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("paused"), "{body}");
    assert_eq!(
        c.list(&alice).await.0,
        StatusCode::OK,
        "still viewable while paused"
    );
    c.set_state("live").await;
    assert_eq!(c.submit(&alice, id, FLAG).await.0, StatusCode::OK);
    let second = c.challenge(json!({ "title": "Two" })).await;
    c.set_state("ended").await;
    let (status, body) = c.submit(&alice, second, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("ended"), "{body}");
    assert_eq!(
        c.list(&alice).await.0,
        StatusCode::OK,
        "still viewable after the end"
    );
}

#[tokio::test]
async fn verdicts_bad_format_and_attempt_cap() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({ "max_attempts": 2 })).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;

    for _ in 0..3 {
        let (status, body) = c.submit(&alice, id, "no braces here").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["verdict"], "bad_format");
        assert_eq!(body["correct"], false);
        assert_eq!(
            body["attempts_remaining"], 2,
            "bad format is not an attempt"
        );
    }
    let (_, body) = c.submit(&alice, id, "KYMO{wrong}").await;
    assert_eq!(body["verdict"], "incorrect");
    assert_eq!(body["attempts_remaining"], 1);
    let (_, body) = c.submit(&alice, id, "KYMO{wrong2}").await;
    assert_eq!(body["attempts_remaining"], 0);
    let (status, body) = c.submit(&alice, id, FLAG).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the cap also blocks the right flag: {body}"
    );
    assert!(body["error"].as_str().unwrap().contains("attempt limit"));
    let (_, list) = c.list(&alice).await;
    assert_eq!(challenge_in(&list, id).unwrap()["attempts_remaining"], 0);

    // A fresh challenge accepts the right flag, padded with spaces, and reports points.
    let other = c.challenge(json!({ "title": "Other" })).await;
    let (status, body) = c.submit(&alice, other, &format!("  {FLAG}  ")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["verdict"], "correct");
    assert_eq!(body["points"], 100);
}

#[tokio::test]
async fn duplicate_solves_are_refused_and_counted_once() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    assert_eq!(c.submit(&alice, id, FLAG).await.1["verdict"], "correct");
    let before = c.history("").await["total"].as_i64().unwrap();
    let (status, body) = c.submit(&alice, id, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        c.history("").await["total"].as_i64().unwrap(),
        before,
        "nothing recorded"
    );
    let (_, board) = c.board(Some(&alice)).await;
    assert_eq!(board["rows"][0]["points"], 100);
    assert_eq!(board["rows"][0]["solves"], 1);
    let (_, shown) = c
        .admin("GET", &format!("/admin/challenges/{id}"), None)
        .await;
    assert_eq!(shown["solves"], 1);
    let (status, _) = c
        .admin("DELETE", &format!("/admin/challenges/{id}"), None)
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "solved challenges are retired, not deleted"
    );
}

#[tokio::test]
async fn rate_limit_returns_429_with_retry_after() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let (status, _) = c
        .admin(
            "PUT",
            &format!("/admin/events/{}/jeopardy", c.event_id),
            Some(json!({ "tie_breaks": ["earliest_last_solve"], "rate_max_attempts": 3, "rate_window_secs": 60, "rate_lockout_secs": 120 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (alice, _) = player(&c.w.app, "alice").await;
    let (bob, _) = player(&c.w.app, "bob").await;
    c.register(&alice).await;
    c.register(&bob).await;
    c.set_state("live").await;

    for _ in 0..6 {
        assert_eq!(
            c.submit(&alice, id, "garbage").await.1["verdict"],
            "bad_format"
        );
    }
    for _ in 0..3 {
        assert_eq!(
            c.submit(&alice, id, "KYMO{no}").await.1["verdict"],
            "incorrect"
        );
    }
    let r = send(
        &c.w.app,
        "POST",
        &format!("/api/v1/events/{}/challenges/{id}/submit", c.slug),
        Some(&alice),
        Some("application/json"),
        json!({ "flag": FLAG }).to_string().into_bytes(),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.headers.get("retry-after").unwrap(), "120");
    assert!(r.json()["error"].is_string());
    // The denied correct flag was not recorded and another participant is unaffected.
    assert_eq!(c.history("verdict=correct").await["total"], 0);
    assert_eq!(c.submit(&bob, id, "KYMO{no}").await.0, StatusCode::OK);

    c.w.set_time(T0 + 121);
    let (status, body) = c.submit(&alice, id, FLAG).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["verdict"], "correct");
}

#[tokio::test]
async fn hint_cost_reduces_points() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let other = c.challenge(json!({ "title": "Other" })).await;
    let (_, hint) = c
        .admin(
            "POST",
            &format!("/admin/challenges/{id}/hints"),
            Some(json!({ "content": "h1", "cost": 30 })),
        )
        .await;
    let (_, foreign) = c
        .admin(
            "POST",
            &format!("/admin/challenges/{other}/hints"),
            Some(json!({ "content": "h2", "cost": 5 })),
        )
        .await;
    let hint_id = hint["id"].as_i64().unwrap();
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    let unlock = format!(
        "/api/v1/events/{}/challenges/{id}/hints/{hint_id}/unlock",
        c.slug
    );

    let (_, first) = call(&c.w.app, "POST", &unlock, Some(&alice), None).await;
    assert_eq!(first["newly_unlocked"], true);
    let (_, again) = call(&c.w.app, "POST", &unlock, Some(&alice), None).await;
    assert_eq!(again["newly_unlocked"], false, "unlocking twice costs once");
    let wrong = format!(
        "/api/v1/events/{}/challenges/{id}/hints/{}/unlock",
        c.slug, foreign["id"]
    );
    assert_eq!(
        call(&c.w.app, "POST", &wrong, Some(&alice), None).await.0,
        StatusCode::NOT_FOUND
    );

    let (_, body) = c.submit(&alice, id, FLAG).await;
    assert_eq!(body["points"], 70);
    let (_, board) = c.board(Some(&alice)).await;
    assert_eq!(board["rows"][0]["points"], 70);
    let (status, progress) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/events/{}/progress", c.slug),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(progress["points"], 70);
    assert_eq!(progress["solves"][0]["hints_cost"], 30);
    assert_eq!(progress["rank"], 1);
    assert_eq!(progress["solved_count"], 1);
    assert_eq!(progress["total_challenges"], 2);
    // No more buying hints on a solved challenge.
    assert_eq!(
        call(&c.w.app, "POST", &unlock, Some(&alice), None).await.0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn dynamic_points_shrink_when_others_solve() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c
        .challenge(json!({ "scoring": { "kind": "dynamic", "initial": 500, "minimum": 100, "decay": 100, "curve": "linear" } }))
        .await;
    let (alice, _) = player(&c.w.app, "alice").await;
    let (bob, _) = player(&c.w.app, "bob").await;
    c.register(&alice).await;
    c.register(&bob).await;
    c.set_state("live").await;

    let (_, first) = c.submit(&alice, id, FLAG).await;
    assert_eq!(first["points"], 500);
    c.w.set_time(T0 + 10);
    let (_, second) = c.submit(&bob, id, FLAG).await;
    assert_eq!(second["points"], 400);
    let (_, board) = c.board(Some(&alice)).await;
    let points: Vec<i64> = board["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["points"].as_i64().unwrap())
        .collect();
    assert_eq!(
        points,
        [400, 400],
        "the first solver is worth the new value too"
    );
    let (_, list) = c.list(&alice).await;
    let shown = challenge_in(&list, id).unwrap();
    assert_eq!(shown["points"], 400);
    assert_eq!(shown["solved_by"], 2);
}

#[tokio::test]
async fn team_events_share_solves_and_hide_breakdowns() {
    let c = Ctx::new("team", json!({ "team_min": 1, "team_max": 3 })).await;
    let one = c.challenge(json!({ "title": "One" })).await;
    let two = c
        .challenge(json!({ "title": "Two", "flag": { "kind": "static", "value": "KYMO{two}" } }))
        .await;
    let (owner, _) = player(&c.w.app, "owner").await;
    let (mate, mate_id) = player(&c.w.app, "mate").await;
    let (rival, _) = player(&c.w.app, "rival").await;
    let team = |name: &str| json!({ "name": name });
    let (_, red) = call(
        &c.w.app,
        "POST",
        "/api/v1/teams",
        Some(&owner),
        Some(team("Red")),
    )
    .await;
    let (_, blue) = call(
        &c.w.app,
        "POST",
        "/api/v1/teams",
        Some(&rival),
        Some(team("Blue")),
    )
    .await;
    let (status, _) = call(
        &c.w.app,
        "POST",
        "/api/v1/teams/join",
        Some(&mate),
        Some(json!({ "invite_code": red["invite_code"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    c.register(&owner).await;
    c.register(&rival).await;
    c.set_state("live").await;
    let red_id = red["id"].as_i64().unwrap();
    let blue_id = blue["id"].as_i64().unwrap();

    // The mate solves for the team, so the owner cannot solve it again.
    let (status, body) = c.submit(&mate, one, FLAG).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["verdict"], "correct");
    let (status, _) = c.submit(&owner, one, FLAG).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, body) = c.submit(&owner, two, "KYMO{two}").await;
    assert_eq!(body["points"], 100);
    // The mate is not separately registered but plays through the team.
    let (_, list) = c.list(&mate).await;
    assert_eq!(challenge_in(&list, one).unwrap()["solved"], true);

    let (_, board) = c.board(Some(&owner)).await;
    assert_eq!(board["rows"][0]["name"], "Red");
    assert_eq!(board["rows"][0]["kind"], "team");
    assert_eq!(board["rows"][0]["points"], 200);
    assert_eq!(board["rows"][1]["name"], "Blue");

    let progress = format!("/api/v1/teams/{red_id}/progress?event={}", c.slug);
    let (status, body) = call(&c.w.app, "GET", &progress, Some(&mate), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["team"]["points"], 200);
    let members = body["members"].as_array().unwrap();
    let mate_row = members.iter().find(|m| m["user_id"] == mate_id).unwrap();
    assert_eq!(mate_row["points"], 100);
    assert_eq!(mate_row["solves"][0]["title"], "One");
    let owner_row = members.iter().find(|m| m["username"] == "owner").unwrap();
    assert_eq!(owner_row["solves"][0]["title"], "Two");

    // Privacy: other teams, outsiders and visitors cannot read it.
    assert_eq!(
        call(&c.w.app, "GET", &progress, Some(&rival), None).await.0,
        StatusCode::NOT_FOUND
    );
    let blue_progress = format!("/api/v1/teams/{blue_id}/progress?event={}", c.slug);
    assert_eq!(
        call(&c.w.app, "GET", &blue_progress, Some(&owner), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&c.w.app, "GET", &progress, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, mine) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/events/{}/progress", c.slug),
        Some(&mate),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mine["kind"], "team");
    assert_eq!(mine["points"], 200);
}

#[tokio::test]
async fn scoreboard_is_hidden_until_published() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    let (eve, _) = player(&c.w.app, "eve").await;
    c.register(&alice).await;
    c.set_state("live").await;
    c.submit(&alice, id, FLAG).await;

    assert_eq!(c.board(None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(c.board(Some(&eve)).await.0, StatusCode::FORBIDDEN);
    let (status, own) = c.board(Some(&alice)).await;
    assert_eq!(status, StatusCode::OK, "registered participants see it");
    assert_eq!(own["public"], false);
    assert_eq!(own["rows"][0]["solved"], json!([id]));
    assert_eq!(own["challenges"][0]["title"], "Warmup");
    assert_eq!(c.board(Some(&c.admin)).await.0, StatusCode::OK);

    let (status, event) = c
        .admin(
            "POST",
            &format!("/admin/events/{}/scoreboard", c.event_id),
            Some(json!({ "public": true })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(event["scoreboard_public"], true);
    let (status, board) = c.board(None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(board["rows"][0]["name"], "alice");
    assert_eq!(board["rows"][0]["avatar_seed"], "alice");
    assert_eq!(c.board(Some(&eve)).await.0, StatusCode::OK);
    // A player cannot publish it.
    let (status, _) = call(
        &c.w.app,
        "POST",
        &format!("/api/v1/admin/events/{}/scoreboard", c.event_id),
        Some(&alice),
        Some(json!({ "public": false })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn tie_breaks_decide_equal_scores() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    let (bob, _) = player(&c.w.app, "bob").await;
    let (carol, _) = player(&c.w.app, "carol").await;
    for t in [&alice, &bob, &carol] {
        c.register(t).await;
    }
    c.set_state("live").await;
    c.submit(&bob, id, FLAG).await;
    c.w.set_time(T0 + 30);
    c.submit(&alice, id, FLAG).await;

    let names = |board: &Value| -> Vec<(String, u64)> {
        board["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["name"].as_str().unwrap().to_owned(),
                    r["rank"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let (_, board) = c.board(Some(&alice)).await;
    assert_eq!(
        names(&board),
        [("bob".into(), 1), ("alice".into(), 2), ("carol".into(), 3)],
        "earliest last solve wins the tie"
    );

    let settings = format!("/admin/events/{}/jeopardy", c.event_id);
    let (status, _) = c.admin("PUT", &settings, Some(json!({ "tie_breaks": [], "rate_max_attempts": 10, "rate_window_secs": 60, "rate_lockout_secs": 60 }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, board) = c.board(Some(&alice)).await;
    let ranks: Vec<u64> = names(&board).iter().map(|n| n.1).collect();
    assert_eq!(
        ranks,
        [1, 1, 3],
        "without tie breaks equal scores share a rank"
    );

    for bad in [
        json!({ "tie_breaks": ["most_solves", "most_solves"], "rate_max_attempts": 10, "rate_window_secs": 60, "rate_lockout_secs": 60 }),
        json!({ "tie_breaks": [], "rate_max_attempts": 0, "rate_window_secs": 60, "rate_lockout_secs": 60 }),
        json!({ "tie_breaks": [], "rate_max_attempts": 10, "rate_window_secs": 0, "rate_lockout_secs": 60 }),
    ] {
        assert_eq!(
            c.admin("PUT", &settings, Some(bad)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let (_, saved) = c.admin("GET", &settings, None).await;
    assert_eq!(saved["tie_breaks"], json!([]));
}

#[tokio::test]
async fn submission_history_filters_and_pages() {
    let c = Ctx::new("solo", json!({})).await;
    let one = c.challenge(json!({ "title": "One" })).await;
    let two = c.challenge(json!({ "title": "Two" })).await;
    let (alice, alice_id) = player(&c.w.app, "alice").await;
    let (bob, bob_id) = player(&c.w.app, "bob").await;
    c.register(&alice).await;
    c.register(&bob).await;
    c.set_state("live").await;
    let post = |token: &str, ch: i64, flag: &str, ip: &'static str| {
        let uri = format!("/api/v1/events/{}/challenges/{ch}/submit", c.slug);
        let token = token.to_owned();
        let body = json!({ "flag": flag }).to_string().into_bytes();
        let app = c.w.app.clone();
        async move {
            send(
                &app,
                "POST",
                &uri,
                Some(&token),
                Some("application/json"),
                body,
                Some(ip),
            )
            .await
        }
    };
    post(&alice, one, "KYMO{x}", "10.0.0.1").await;
    post(&alice, one, FLAG, "10.0.0.1").await;
    post(&bob, two, "bad", "10.0.0.2").await;
    post(&bob, two, "KYMO{y}", "10.0.0.2").await;
    post(&bob, one, FLAG, "10.0.0.2").await;

    let all = c.history("").await;
    assert_eq!(all["total"], 5);
    assert_eq!(all["items"][0]["verdict"], "correct", "newest first");
    assert_eq!(all["items"][0]["username"], "bob");
    assert_eq!(c.history(&format!("user_id={alice_id}")).await["total"], 2);
    assert_eq!(
        c.history(&format!("user_id={bob_id}&challenge_id={two}"))
            .await["total"],
        2
    );
    assert_eq!(c.history("verdict=correct").await["total"], 2);
    assert_eq!(c.history("verdict=bad_format").await["total"], 1);
    assert_eq!(c.history("ip=10.0.0.1").await["total"], 2);
    assert_eq!(c.history("ip=10.0.0.2&verdict=incorrect").await["total"], 1);
    assert_eq!(c.history("ip=10.9.9.9").await["total"], 0);
    let page = c.history("per_page=2&page=3").await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["total"], 5);
    assert_eq!(page["items"][0]["value"], "KYMO{x}");
    assert_eq!(page["items"][0]["challenge_title"], "One");
    let (status, _) = c
        .admin(
            "GET",
            &format!("/admin/events/{}/submissions?verdict=maybe", c.event_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &c.w.app,
        "GET",
        &format!("/api/v1/admin/events/{}/submissions", c.event_id),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "players cannot read the history"
    );
}

#[tokio::test]
async fn the_test_endpoint_records_nothing() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let regex = c
        .challenge(
            json!({ "title": "Regex", "flag": { "kind": "regex", "pattern": "KYMO\\{[a-z]+\\}" } }),
        )
        .await;
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;

    let test = |ch: i64, flag: &'static str| {
        let c = &c;
        async move {
            c.admin(
                "POST",
                &format!("/admin/challenges/{ch}/test"),
                Some(json!({ "flag": flag })),
            )
            .await
        }
    };
    assert_eq!(test(id, FLAG).await.1["verdict"], "correct");
    assert_eq!(test(id, "KYMO{no}").await.1["verdict"], "incorrect");
    assert_eq!(test(id, "plain").await.1["verdict"], "bad_format");
    assert_eq!(test(regex, "KYMO{abc}").await.1["verdict"], "correct");
    assert_eq!(test(regex, "KYMO{ABC}").await.1["verdict"], "incorrect");

    assert_eq!(c.history("").await["total"], 0);
    let (_, shown) = c
        .admin("GET", &format!("/admin/challenges/{id}"), None)
        .await;
    assert_eq!(shown["solves"], 0);
    let (_, list) = c.list(&alice).await;
    assert_eq!(challenge_in(&list, id).unwrap()["solved"], false);
    let (status, _) = call(
        &c.w.app,
        "POST",
        &format!("/api/v1/admin/challenges/{id}/test"),
        Some(&alice),
        Some(json!({ "flag": FLAG })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn attachments_upload_and_download_safely() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({})).await;
    let hidden = c.draft(json!({ "title": "Hidden" })).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    let (eve, _) = player(&c.w.app, "eve").await;
    c.register(&alice).await;
    c.set_state("live").await;

    let upload = format!("/api/v1/admin/challenges/{id}/attachments?name=..%2F..%2Fmy%20file.txt");
    let r = send(
        &c.w.app,
        "POST",
        &upload,
        Some(&c.admin),
        Some("text/html; charset=utf-8"),
        b"<b>hello</b>".to_vec(),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json());
    let meta = r.json();
    assert_eq!(meta["name"], "my_file.txt", "path and spaces are sanitised");
    assert_eq!(meta["mime"], "text/html");
    assert_eq!(meta["size"], 12);
    assert_eq!(meta["sha256"].as_str().unwrap().len(), 64);
    let raw_id = meta["id"].as_i64().unwrap();

    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(b"\x00\x01binary");
    let (status, json_meta) = call(
        &c.w.app,
        "POST",
        &format!("/api/v1/admin/challenges/{id}/attachments"),
        Some(&c.admin),
        Some(json!({ "name": "a.bin", "mime": "not a mime", "data_base64": encoded })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json_meta}");
    assert_eq!(
        json_meta["mime"], "application/octet-stream",
        "bad media types fall back"
    );
    assert_eq!(json_meta["size"], 8);

    let (status, _) = call(
        &c.w.app,
        "POST",
        &format!("/api/v1/admin/challenges/{id}/attachments"),
        Some(&c.admin),
        Some(json!({ "name": "a.bin", "data_base64": "###" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let big = vec![7u8; 10 * 1024 * 1024 + 1];
    let r = send(
        &c.w.app,
        "POST",
        &upload,
        Some(&c.admin),
        Some("application/octet-stream"),
        big,
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "over 10 MB");
    let ok = vec![7u8; 10 * 1024 * 1024];
    let r = send(
        &c.w.app,
        "POST",
        &upload,
        Some(&c.admin),
        Some("application/octet-stream"),
        ok,
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "exactly 10 MB is accepted");
    let r = send(
        &c.w.app,
        "POST",
        &upload,
        Some(&c.admin),
        Some("text/plain"),
        Vec::new(),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "empty file");
    let r = send(
        &c.w.app,
        "POST",
        &upload,
        Some(&alice),
        Some("text/plain"),
        b"x".to_vec(),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    let (_, list) = c.list(&alice).await;
    let files = challenge_in(&list, id).unwrap()["attachments"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(files.len(), 3);
    let url = files[0]["url"].as_str().unwrap().to_owned();
    assert_eq!(
        url,
        format!(
            "/api/v1/events/{}/challenges/{id}/attachments/{raw_id}",
            c.slug
        )
    );
    let r = send(&c.w.app, "GET", &url, Some(&alice), None, Vec::new(), None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.bytes, b"<b>hello</b>");
    assert_eq!(
        r.headers.get("content-disposition").unwrap(),
        "attachment; filename=\"my_file.txt\""
    );
    assert_eq!(r.headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(r.headers.get("content-type").unwrap(), "text/html");

    let r = send(&c.w.app, "GET", &url, Some(&eve), None, Vec::new(), None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "not registered");
    let r = send(&c.w.app, "GET", &url, None, None, Vec::new(), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let wrong = format!(
        "/api/v1/events/{}/challenges/{hidden}/attachments/{raw_id}",
        c.slug
    );
    let r = send(
        &c.w.app,
        "GET",
        &wrong,
        Some(&alice),
        None,
        Vec::new(),
        None,
    )
    .await;
    assert_eq!(
        r.status,
        StatusCode::NOT_FOUND,
        "drafts and mismatched ids are 404"
    );

    let (status, _) = c
        .admin("DELETE", &format!("/admin/attachments/{raw_id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let r = send(&c.w.app, "GET", &url, Some(&alice), None, Vec::new(), None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn correct_flags_are_masked_for_staff_and_revealed_one_at_a_time_with_an_audit_entry() {
    let c = Ctx::new("solo", json!({})).await;
    let id = c.challenge(json!({ "title": "Secret" })).await;
    let (alice, _) = player(&c.w.app, "alice").await;
    c.register(&alice).await;
    c.set_state("live").await;
    c.submit(&alice, id, "KYMO{wrong_guess}").await;
    let (status, solved) = c.submit(&alice, id, FLAG).await;
    assert_eq!(status, StatusCode::OK, "{solved}");

    let page = c.history("").await;
    let items = page["items"].as_array().unwrap();
    let correct = items.iter().find(|i| i["verdict"] == "correct").unwrap();
    assert_eq!(correct["value"], "", "a correct flag is never listed");
    assert_eq!(correct["hidden"], true);
    let wrong = items.iter().find(|i| i["verdict"] == "incorrect").unwrap();
    assert_eq!(
        wrong["value"], "KYMO{wrong_guess}",
        "wrong guesses stay readable"
    );
    assert_eq!(wrong["hidden"], false);
    assert!(
        !page.to_string().contains(FLAG),
        "the flag is nowhere in the list"
    );

    // The reveal is a separate call, refused to players and written to the audit trail.
    let uri = format!(
        "/api/v1/admin/events/{}/submissions/{}/value",
        c.event_id, correct["id"]
    );
    let (status, _) = call(&c.w.app, "GET", &uri, Some(&alice), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, shown) = call(&c.w.app, "GET", &uri, Some(&c.admin), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shown["value"], FLAG);
    let (_, audit) = call(
        &c.w.app,
        "GET",
        "/api/v1/admin/monitor/audit?action=reveal_submission",
        Some(&c.admin),
        None,
    )
    .await;
    assert_eq!(audit["total"], 1, "{audit}");
}

#[tokio::test]
async fn the_score_timeline_ends_on_the_board_totals_and_follows_the_same_visibility() {
    let c = Ctx::new("solo", json!({})).await;
    let one = c.challenge(json!({ "title": "One" })).await;
    let two = c
        .challenge(json!({ "title": "Two", "flag": { "kind": "static", "value": "KYMO{two}" } }))
        .await;
    let (alice, _) = player(&c.w.app, "alice").await;
    let (bob, _) = player(&c.w.app, "bob").await;
    let (idle, _) = player(&c.w.app, "idle").await;
    for t in [&alice, &bob, &idle] {
        c.register(t).await;
    }
    c.set_state("live").await;
    c.submit(&alice, one, FLAG).await;
    c.submit(&bob, one, FLAG).await;
    c.submit(&alice, two, "KYMO{two}").await;

    let uri = format!("/api/v1/events/{}/scoreboard/timeline", c.slug);
    // Not public yet: the chart is as hidden as the table.
    assert_eq!(
        call(&c.w.app, "GET", &uri, None, None).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, own) = call(&c.w.app, "GET", &uri, Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "registered players see it: {own}");
    c.admin(
        "POST",
        &format!("/admin/events/{}/scoreboard", c.event_id),
        Some(json!({ "public": true })),
    )
    .await;

    let (status, chart) = call(&c.w.app, "GET", &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{chart}");
    let (_, board) = c.board(None).await;
    let series = chart["series"].as_array().unwrap();
    assert_eq!(series.len(), 2, "a player with no solve has no line");
    for line in series {
        let row = board["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["participant_id"] == line["participant_id"])
            .unwrap();
        let steps = line["steps"].as_array().unwrap();
        assert_eq!(steps.last().unwrap()["total"], row["points"], "{line}");
        assert_eq!(line["rank"], row["rank"]);
    }
    assert_eq!(series[0]["name"], "alice", "best first");
    assert!(chart["from"].as_i64().unwrap() <= chart["to"].as_i64().unwrap());

    // The number of lines can be limited, and the limit is bounded.
    let (_, one_line) = call(&c.w.app, "GET", &format!("{uri}?top=1"), None, None).await;
    assert_eq!(one_line["series"].as_array().unwrap().len(), 1);
    let (status, _) = call(&c.w.app, "GET", &format!("{uri}?top=9999"), None, None).await;
    assert_eq!(status, StatusCode::OK);
}
