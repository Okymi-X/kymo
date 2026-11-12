use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;
const HOUR: i64 = 3600;

struct World {
    app: Router,
    clock: Arc<AtomicI64>,
    pool: SqlitePool,
}

impl World {
    fn set_time(&self, t: i64) {
        self.clock.store(t, Ordering::SeqCst);
    }
}

async fn world() -> World {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    with_pool(pool)
}

fn with_pool(pool: SqlitePool) -> World {
    let clock = Arc::new(AtomicI64::new(T0));
    let shared = Arc::clone(&clock);
    let state = AppState::new(pool.clone(), PASSWORD)
        .unwrap()
        .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
    World {
        app: router(state),
        clock,
        pool,
    }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
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

async fn make_event(app: &Router, admin: &str, extra: Value) -> Value {
    let mut body = json!({ "name": "Spring CTF", "kind": "jeopardy", "mode": "solo" });
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let (status, event) = call(app, "POST", "/api/v1/admin/events", Some(admin), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{event}");
    event
}

async fn set_state(app: &Router, admin: &str, id: i64, state: &str) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/api/v1/admin/events/{id}/state"),
        Some(admin),
        Some(json!({ "state": state })),
    )
    .await
}

#[tokio::test]
async fn backfill_creates_an_event_per_existing_tournament() {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let migrator = sqlx::migrate!();
    let mut before = sqlx::migrate!();
    before.migrations = migrator
        .migrations
        .iter()
        .filter(|m| m.version <= 14)
        .cloned()
        .collect();
    before.run(&pool).await.unwrap();
    for (name, slug, status) in [
        ("Old Draft", "old-draft", "draft"),
        ("Old Live", "old-live", "live"),
        ("Old Done", "old-done", "done"),
    ] {
        sqlx::query(
            "INSERT INTO tournament (name, slug, format, status) VALUES (?, ?, 'single_elimination', ?)",
        )
        .bind(name)
        .bind(slug)
        .bind(status)
        .execute(&pool)
        .await
        .unwrap();
    }
    migrator.run(&pool).await.unwrap();

    let rows: Vec<(String, String, String, String, bool)> = sqlx::query_as(
        "SELECT e.slug, e.kind, e.mode, e.state, t.event_id = e.id FROM tournament t JOIN event e ON e.id = t.event_id ORDER BY t.id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let states: Vec<_> = rows.iter().map(|r| (r.0.as_str(), r.3.as_str())).collect();
    assert_eq!(
        states,
        [
            ("old-draft", "draft"),
            ("old-live", "live"),
            ("old-done", "ended")
        ]
    );
    assert!(
        rows.iter()
            .all(|r| r.1 == "tournament" && r.2 == "team" && r.4)
    );

    let w = with_pool(pool);
    let (viewer, _) = player(&w.app, "viewer").await;
    let (status, list) = call(&w.app, "GET", "/api/v1/events", Some(&viewer), None).await;
    assert_eq!(status, StatusCode::OK);
    let slugs: Vec<_> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["old-done", "old-live"], "drafts stay hidden");
    let (_, tournament) = call(
        &w.app,
        "GET",
        "/api/v1/tournaments/old-live",
        Some(&viewer),
        None,
    )
    .await;
    assert!(tournament["event_id"].is_i64());
}

#[tokio::test]
async fn new_tournaments_get_an_event_with_a_free_slug() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    make_event(&w.app, &admin, json!({ "name": "Spring Cup" })).await;
    let (status, t) = call(
        &w.app,
        "POST",
        "/api/v1/tournaments",
        Some(&admin),
        Some(json!({"name": "Spring Cup", "format": "single_elimination"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(t["slug"], "spring-cup");
    let slug: String = sqlx::query_scalar("SELECT slug FROM event WHERE id = ?")
        .bind(t["event_id"].as_i64().unwrap())
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(slug, "spring-cup-2");
    let id = t["id"].as_i64().unwrap();
    let (status, _) = call(
        &w.app,
        "DELETE",
        &format!("/api/v1/admin/tournaments/{id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event WHERE slug = 'spring-cup-2'")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "deleting a tournament removes its event");
}

#[tokio::test]
async fn register_login_and_me() {
    let w = world().await;
    let (token, _) = player(&w.app, "Alice_1").await;

    let (status, me) = call(&w.app, "GET", "/api/v1/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["username"], "Alice_1");
    assert_eq!(me["role"], "player");
    assert!(me.get("password_hash").is_none());

    // Usernames are unique ignoring case.
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({ "username": "alice_1", "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    for bad in [
        json!({ "username": "ab", "password": PLAYER_PASSWORD }),
        json!({ "username": "has space", "password": PLAYER_PASSWORD }),
        json!({ "username": "zero\u{200B}width", "password": PLAYER_PASSWORD }),
        json!({ "username": "shortpass", "password": "123456789" }),
        json!({ "username": "bademail", "password": PLAYER_PASSWORD, "email": "nope" }),
    ] {
        let (status, _) = call(&w.app, "POST", "/api/v1/auth/register", None, Some(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    let (status, login) = call(
        &w.app,
        "POST",
        "/api/v1/auth/player-login",
        None,
        Some(json!({ "username": "ALICE_1", "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second = login["token"].as_str().unwrap().to_owned();
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/player-login",
        None,
        Some(json!({ "username": "alice_1", "password": "wrong password!" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Profile edits, and the password change needs the current password.
    let (status, me) = call(
        &w.app,
        "PUT",
        "/api/v1/me",
        Some(&second),
        Some(json!({ "display_name": "Alice", "bio": "hello" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["display_name"], "Alice");
    let (status, _) = call(
        &w.app,
        "PUT",
        "/api/v1/me",
        Some(&second),
        Some(json!({ "current_password": "not the password", "new_password": "another long password" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &w.app,
        "PUT",
        "/api/v1/me",
        Some(&second),
        Some(
            json!({ "current_password": PLAYER_PASSWORD, "new_password": "another long password" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&w.app, "GET", "/api/v1/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "other sessions end");
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/player-login",
        None,
        Some(json!({ "username": "alice_1", "password": "another long password" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call(&w.app, "POST", "/api/v1/auth/logout", Some(&second), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&w.app, "GET", "/api/v1/me", Some(&second), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn registration_can_be_disabled_by_the_admin() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (status, _) = call(
        &w.app,
        "PUT",
        "/api/v1/admin/registration",
        Some(&admin),
        Some(json!({ "open": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({ "username": "latecomer", "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn banned_users_cannot_log_in_and_lose_their_sessions() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (token, id) = player(&w.app, "troll").await;
    let (status, user) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/users/{id}"),
        Some(&admin),
        Some(json!({ "banned": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(user["banned"], true);
    let (status, _) = call(&w.app, "GET", "/api/v1/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/player-login",
        None,
        Some(json!({ "username": "troll", "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Unbanning restores access.
    call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/users/{id}"),
        Some(&admin),
        Some(json!({ "banned": false })),
    )
    .await;
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/player-login",
        None,
        Some(json!({ "username": "troll", "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn roles_gate_the_admin_routes() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (player_token, player_id) = player(&w.app, "plain").await;
    let (boss_token, boss_id) = player(&w.app, "boss").await;
    let (ref_token, ref_id) = player(&w.app, "ref").await;

    let admin_routes: [(&str, &str, Option<Value>); 5] = [
        ("GET", "/api/v1/admin/users", None),
        ("GET", "/api/v1/admin/events", None),
        ("GET", "/api/v1/admin/tokens", None),
        ("GET", "/api/v1/admin/registration", None),
        (
            "POST",
            "/api/v1/tournaments",
            Some(json!({"name": "x", "format": "single_elimination"})),
        ),
    ];
    for (method, uri, body) in &admin_routes {
        let (status, _) = call(&w.app, method, uri, Some(&player_token), body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "player on {uri}");
    }

    for (id, role) in [(boss_id, "admin"), (ref_id, "referee")] {
        let (status, _) = call(
            &w.app,
            "PUT",
            &format!("/api/v1/admin/users/{id}"),
            Some(&admin),
            Some(json!({ "role": role })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    for (method, uri, body) in &admin_routes {
        let (status, _) = call(&w.app, method, uri, Some(&boss_token), body.clone()).await;
        assert!(status.is_success(), "user admin on {uri}: {status}");
    }
    // Admins may mint API tokens from a user session, a referee account may not.
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/admin/tokens",
        Some(&boss_token),
        Some(json!({ "label": "ci" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    for (method, uri, body) in &admin_routes {
        let (status, _) = call(&w.app, method, uri, Some(&ref_token), body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "referee on {uri}");
    }
    // A referee account has no tournament scope yet, so it cannot record results.
    let (_, t) = call(
        &w.app,
        "POST",
        "/api/v1/tournaments",
        Some(&admin),
        Some(json!({"name": "Scoped", "format": "single_elimination"})),
    )
    .await;
    let tid = t["id"].as_i64().unwrap();
    let (status, _) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/tournaments/{tid}/matches/1/schedule"),
        Some(&ref_token),
        Some(json!({"scheduled_at": null})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/tournaments/{tid}/matches/1/schedule"),
        Some(&player_token),
        Some(json!({"scheduled_at": null})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // You cannot demote yourself, and the legacy password admin is not a player account.
    let (status, _) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/users/{boss_id}"),
        Some(&boss_token),
        Some(json!({ "role": "player" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = call(&w.app, "GET", "/api/v1/me", Some(&admin), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(&w.app, "GET", "/api/v1/admin/users", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Paginated search.
    let (status, page) = call(
        &w.app,
        "GET",
        "/api/v1/admin/users?q=bo&per_page=1",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["username"], "boss");
    let _ = player_id;
}

#[tokio::test]
async fn event_states_follow_the_state_machine() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let event = make_event(&w.app, &admin, json!({})).await;
    let id = event["id"].as_i64().unwrap();
    assert_eq!(event["state"], "draft");

    // Drafts are hidden from the public, and a visitor has to sign in to ask at all.
    let (status, _) = call(&w.app, "GET", "/api/v1/events/spring-ctf", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (viewer, _) = player(&w.app, "viewer").await;
    let (status, _) = call(
        &w.app,
        "GET",
        "/api/v1/events/spring-ctf",
        Some(&viewer),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = set_state(&w.app, &admin, id, "ended").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("draft"));
    let (status, _) = set_state(&w.app, &admin, id, "bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    for state in ["registration_open", "live", "paused", "live", "ended"] {
        let (status, body) = set_state(&w.app, &admin, id, state).await;
        assert_eq!(status, StatusCode::OK, "{state}: {body}");
        assert_eq!(body["state"], state);
    }
    // Only draft and archived events can be deleted.
    let uri = format!("/api/v1/admin/events/{id}");
    let (status, _) = call(&w.app, "DELETE", &uri, Some(&admin), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    set_state(&w.app, &admin, id, "archived").await;
    let (status, _) = call(
        &w.app,
        "GET",
        "/api/v1/events/spring-ctf",
        Some(&viewer),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, public) = call(&w.app, "GET", "/api/v1/events", Some(&viewer), None).await;
    assert_eq!(
        public.as_array().unwrap().len(),
        0,
        "archived leaves the list"
    );
    let (status, _) = call(&w.app, "DELETE", &uri, Some(&admin), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn effective_state_follows_the_clock() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let event = make_event(
        &w.app,
        &admin,
        json!({ "starts_at": T0 + HOUR, "ends_at": T0 + 3 * HOUR }),
    )
    .await;
    let id = event["id"].as_i64().unwrap();
    set_state(&w.app, &admin, id, "registration_open").await;
    let (viewer, _) = player(&w.app, "viewer").await;

    let effective = |at: i64| {
        let w = &w;
        let viewer = viewer.clone();
        async move {
            w.set_time(at);
            let (_, e) = call(
                &w.app,
                "GET",
                "/api/v1/events/spring-ctf",
                Some(&viewer),
                None,
            )
            .await;
            assert_eq!(e["state"], "registration_open", "stored state is untouched");
            e["effective_state"].as_str().unwrap().to_owned()
        }
    };
    assert_eq!(effective(T0).await, "registration_open");
    assert_eq!(effective(T0 + HOUR).await, "live");
    assert_eq!(effective(T0 + 3 * HOUR).await, "ended");

    // Registration obeys the same clock: closed once the event is live (no late window).
    let (token, _) = player(&w.app, "early").await;
    w.set_time(T0);
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/events/spring-ctf/register",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    w.set_time(T0 + 2 * HOUR);
    let (status, body) = call(
        &w.app,
        "DELETE",
        "/api/v1/events/spring-ctf/register",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

#[tokio::test]
async fn solo_registration_and_mode_lock() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let event = make_event(&w.app, &admin, json!({})).await;
    let id = event["id"].as_i64().unwrap();
    let (token, user_id) = player(&w.app, "solo").await;
    let reg = "/api/v1/events/spring-ctf/register";

    let (status, _) = call(&w.app, "POST", reg, Some(&token), None).await;
    assert_eq!(status, StatusCode::CONFLICT, "draft is not open");
    set_state(&w.app, &admin, id, "registration_open").await;
    let (status, _) = call(&w.app, "POST", reg, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(&w.app, "POST", reg, Some(&token), None).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = call(&w.app, "POST", reg, Some(&token), None).await;
    assert_eq!(status, StatusCode::CONFLICT, "twice");

    let (_, list) = call(
        &w.app,
        "GET",
        &format!("/api/v1/admin/events/{id}/participants"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(list[0]["user_id"], user_id);

    let (status, _) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/events/{id}"),
        Some(&admin),
        Some(json!({ "mode": "team" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, updated) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/events/{id}"),
        Some(&admin),
        Some(json!({ "description": "new text", "starts_at": T0 + HOUR })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["description"], "new text");
    let (status, _) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/events/{id}"),
        Some(&admin),
        Some(json!({ "starts_at": T0 + HOUR, "ends_at": T0 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call(&w.app, "DELETE", reg, Some(&token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&w.app, "DELETE", reg, Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn team_registration_roster_limits_invites_and_privacy() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let event = make_event(
        &w.app,
        &admin,
        json!({ "name": "Team Cup", "mode": "team", "team_min": 2, "team_max": 3 }),
    )
    .await;
    let id = event["id"].as_i64().unwrap();
    set_state(&w.app, &admin, id, "registration_open").await;
    let reg = "/api/v1/events/team-cup/register";

    let (owner, owner_id) = player(&w.app, "owner").await;
    let (bob, bob_id) = player(&w.app, "bob").await;
    let (carol, _) = player(&w.app, "carol").await;
    let (dave, _) = player(&w.app, "dave").await;
    let (eve, _) = player(&w.app, "eve").await;

    // No team yet.
    let (status, _) = call(&w.app, "POST", reg, Some(&owner), None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, team) = call(
        &w.app,
        "POST",
        "/api/v1/teams",
        Some(&owner),
        Some(json!({ "name": "Red Team" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let team_id = team["id"].as_i64().unwrap();
    let code = team["invite_code"].as_str().unwrap().to_owned();
    assert_eq!(team["owner_user_id"], owner_id);
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/teams",
        Some(&owner),
        Some(json!({ "name": "Other" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "one team per user");

    // One member is below the minimum of 2.
    let (status, body) = call(&w.app, "POST", reg, Some(&owner), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("2 to 3"));

    // Invite code join; a bad code is unknown.
    let join = |token: String, code: String| {
        let app = w.app.clone();
        async move {
            call(
                &app,
                "POST",
                "/api/v1/teams/join",
                Some(&token),
                Some(json!({ "invite_code": code })),
            )
            .await
        }
    };
    assert_eq!(
        join(bob.clone(), "nope".into()).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, joined) = join(bob.clone(), code.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(joined["members"].as_array().unwrap().len(), 2);
    assert!(
        joined["invite_code"].is_null(),
        "only the owner sees the code"
    );
    assert_eq!(
        join(bob.clone(), code.clone()).await.0,
        StatusCode::CONFLICT
    );

    // Privacy: members read the team, outsiders get 404.
    let team_uri = format!("/api/v1/teams/{team_id}");
    assert_eq!(
        call(&w.app, "GET", &team_uri, Some(&bob), None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&w.app, "GET", &team_uri, Some(&eve), None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&w.app, "GET", &team_uri, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, mine) = call(&w.app, "GET", "/api/v1/teams/mine", Some(&bob), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mine["id"], team_id);
    assert_eq!(
        call(&w.app, "GET", "/api/v1/teams/mine", Some(&eve), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // Only the owner registers the team.
    assert_eq!(
        call(&w.app, "POST", reg, Some(&bob), None).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(&w.app, "POST", reg, Some(&owner), None).await.0,
        StatusCode::CREATED
    );

    // Third member fits, fourth does not (max 3).
    assert_eq!(join(carol.clone(), code.clone()).await.0, StatusCode::OK);
    let (status, body) = join(dave.clone(), code.clone()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("full"));

    // Rotating the code (owner only) invalidates the old one.
    let rotate = format!("/api/v1/teams/{team_id}/invite-code/rotate");
    assert_eq!(
        call(&w.app, "POST", &rotate, Some(&bob), None).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, rotated) = call(&w.app, "POST", &rotate, Some(&owner), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(rotated["invite_code"], code.as_str());

    // Kicking: owner only, then the seat is free again.
    let kick = format!("/api/v1/teams/{team_id}/members/{bob_id}");
    assert_eq!(
        call(&w.app, "DELETE", &kick, Some(&carol), None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&w.app, "DELETE", &kick, Some(&owner), None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&w.app, "GET", &team_uri, Some(&bob), None).await.0,
        StatusCode::NOT_FOUND
    );
    let new_code = rotated["invite_code"].as_str().unwrap().to_owned();
    assert_eq!(join(dave.clone(), new_code.clone()).await.0, StatusCode::OK);

    // Leaving works while registration is open.
    let leave = format!("/api/v1/teams/{team_id}/leave");
    assert_eq!(
        call(&w.app, "POST", &leave, Some(&dave), None).await.0,
        StatusCode::NO_CONTENT
    );

    // Once the event is live the roster is locked for joins, leaves and kicks.
    set_state(&w.app, &admin, id, "live").await;
    assert_eq!(
        join(eve.clone(), new_code.clone()).await.0,
        StatusCode::CONFLICT
    );
    let (status, body) = call(&w.app, "POST", &leave, Some(&carol), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("locked"));
    // Participants lists the team with its size.
    let (_, list) = call(
        &w.app,
        "GET",
        &format!("/api/v1/admin/events/{id}/participants"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(list[0]["team_id"], team_id);
    assert_eq!(list[0]["members"], 2);
}

#[tokio::test]
async fn disabled_invites_and_last_member_leaving() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (owner, _) = player(&w.app, "solitary").await;
    let (guest, _) = player(&w.app, "guest").await;
    let (_, team) = call(
        &w.app,
        "POST",
        "/api/v1/teams",
        Some(&owner),
        Some(json!({ "name": "Duo" })),
    )
    .await;
    let code = team["invite_code"].as_str().unwrap().to_owned();

    let event = make_event(
        &w.app,
        &admin,
        json!({ "name": "No Invites", "mode": "team", "team_min": 1, "allow_invites": false }),
    )
    .await;
    set_state(
        &w.app,
        &admin,
        event["id"].as_i64().unwrap(),
        "registration_open",
    )
    .await;
    assert_eq!(
        call(
            &w.app,
            "POST",
            "/api/v1/events/no-invites/register",
            Some(&owner),
            None
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, body) = call(
        &w.app,
        "POST",
        "/api/v1/teams/join",
        Some(&guest),
        Some(json!({ "invite_code": code })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("invites"));

    // Withdraw, then the sole owner leaves and the team disappears.
    call(
        &w.app,
        "DELETE",
        "/api/v1/events/no-invites/register",
        Some(&owner),
        None,
    )
    .await;
    let team_id = team["id"].as_i64().unwrap();
    assert_eq!(
        call(
            &w.app,
            "POST",
            &format!("/api/v1/teams/{team_id}/leave"),
            Some(&owner),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM player_team")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn the_event_page_says_whether_the_caller_is_registered() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let event = make_event(&w.app, &admin, json!({})).await;
    set_state(
        &w.app,
        &admin,
        event["id"].as_i64().unwrap(),
        "registration_open",
    )
    .await;
    let (token, _) = player(&w.app, "reg").await;
    let (other, _) = player(&w.app, "other").await;
    let uri = "/api/v1/events/spring-ctf";

    // A visitor has to sign in to open the event at all.
    let (status, _) = call(&w.app, "GET", uri, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, before) = call(&w.app, "GET", uri, Some(&token), None).await;
    assert_eq!(before["registered"], false);

    let (status, _) = call(
        &w.app,
        "POST",
        &format!("{uri}/register"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, after) = call(&w.app, "GET", uri, Some(&token), None).await;
    assert_eq!(after["registered"], true);
    assert_eq!(
        after["slug"], "spring-ctf",
        "the event fields are still there"
    );
    let (_, elsewhere) = call(&w.app, "GET", uri, Some(&other), None).await;
    assert_eq!(elsewhere["registered"], false, "registration is per player");

    let (status, _) = call(&w.app, "GET", uri, Some("not-a-token"), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a stale session is not a visitor"
    );
}

#[tokio::test]
async fn closed_sign_ups_say_so_and_the_public_status_matches() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (status, open) = call(&w.app, "GET", "/api/v1/registration", None, None).await;
    assert_eq!(
        (status, open["open"].clone()),
        (StatusCode::OK, json!(true)),
        "open by default, no sign in needed"
    );

    let (status, _) = call(
        &w.app,
        "PUT",
        "/api/v1/admin/registration",
        Some(&admin),
        Some(json!({ "open": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, closed) = call(&w.app, "GET", "/api/v1/registration", None, None).await;
    assert_eq!(closed["open"], false);
    let (status, body) = call(
        &w.app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({ "username": "latecomer", "password": "a long player password" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let message = body["error"].as_str().unwrap();
    assert!(
        message.contains("sign-ups are closed") && message.contains("organizer"),
        "{message}"
    );
    assert!(
        !message.contains("token"),
        "the message is about sign-ups, not tokens: {message}"
    );
}

#[tokio::test]
async fn permission_errors_say_what_is_wrong_in_plain_words() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (player_token, _) = player(&w.app, "plain").await;
    // Not an admin route for a player: the generic message must not mention tokens.
    let (status, body) = call(
        &w.app,
        "GET",
        "/api/v1/admin/users",
        Some(&player_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let message = body["error"].as_str().unwrap();
    assert!(
        message.contains("permission") && !message.contains("token"),
        "{message}"
    );
    // A wrong current password is not "forbidden" in the abstract.
    let (status, body) = call(
        &w.app,
        "PUT",
        "/api/v1/me",
        Some(&player_token),
        Some(json!({ "current_password": "not the password", "new_password": "another long password" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "the current password is not right");
    let _ = admin;
}

#[tokio::test]
async fn the_organizer_contact_is_public_but_only_an_admin_can_change_it() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (_, none) = call(&w.app, "GET", "/api/v1/registration", None, None).await;
    assert_eq!(none["contact"], "");

    let (player_token, _) = player(&w.app, "nosy2").await;
    let (status, _) = call(
        &w.app,
        "PUT",
        "/api/v1/admin/contact",
        Some(&player_token),
        Some(json!({ "contact": "me" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, saved) = call(
        &w.app,
        "PUT",
        "/api/v1/admin/contact",
        Some(&admin),
        Some(json!({ "contact": "  Ask Sam on organizers@example.org  " })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["contact"], "Ask Sam on organizers@example.org");
    let (_, shown) = call(&w.app, "GET", "/api/v1/registration", None, None).await;
    assert_eq!(shown["contact"], "Ask Sam on organizers@example.org");

    for bad in ["two\nlines".to_owned(), "x".repeat(201)] {
        let (status, _) = call(
            &w.app,
            "PUT",
            "/api/v1/admin/contact",
            Some(&admin),
            Some(json!({ "contact": bad })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn an_admin_can_create_accounts_while_sign_ups_stay_closed() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    call(
        &w.app,
        "PUT",
        "/api/v1/admin/registration",
        Some(&admin),
        Some(json!({ "open": false })),
    )
    .await;

    // Without a password the server makes one and shows it once.
    let (status, made) = call(
        &w.app,
        "POST",
        "/api/v1/admin/users",
        Some(&admin),
        Some(json!({ "username": "walkin", "email": "walkin@example.org" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["user"]["role"], "player");
    let temporary = made["temporary_password"].as_str().unwrap().to_owned();
    let (status, session) = call(
        &w.app,
        "POST",
        "/api/v1/auth/player-login",
        None,
        Some(json!({ "username": "walkin", "password": temporary })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{session}");

    // With a password and a role it uses them and returns no temporary password.
    let (status, made) = call(
        &w.app,
        "POST",
        "/api/v1/admin/users",
        Some(&admin),
        Some(json!({ "username": "judge", "role": "referee", "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["user"]["role"], "referee");
    assert!(made["temporary_password"].is_null());

    // Duplicates, bad roles and non admins are refused, and public sign-up is still closed.
    for (body, expected) in [
        (json!({ "username": "walkin" }), StatusCode::CONFLICT),
        (
            json!({ "username": "someone", "role": "visitor" }),
            StatusCode::BAD_REQUEST,
        ),
        (json!({ "username": "a b" }), StatusCode::BAD_REQUEST),
    ] {
        let (status, _) = call(
            &w.app,
            "POST",
            "/api/v1/admin/users",
            Some(&admin),
            Some(body),
        )
        .await;
        assert_eq!(status, expected);
    }
    let player_token = session["token"].as_str().unwrap().to_owned();
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/admin/users",
        Some(&player_token),
        Some(json!({ "username": "sneaky" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({ "username": "latecomer", "password": PLAYER_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_admin_edits_a_profile_and_resets_a_password() {
    let w = world().await;
    let admin = admin_token(&w.app).await;
    let (token, id) = player(&w.app, "dana").await;
    let (status, body) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/users/{id}"),
        Some(&admin),
        Some(json!({ "display_name": "Dana K", "bio": "Forensics", "email": "Dana@Example.org", "new_password": "a brand new password" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        (body["display_name"].as_str(), body["email"].as_str()),
        (Some("Dana K"), Some("dana@example.org"))
    );
    // The old session is gone and the new password works.
    let (status, _) = call(&w.app, "GET", "/api/v1/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        &w.app,
        "POST",
        "/api/v1/auth/player-login",
        None,
        Some(json!({ "username": "dana", "password": "a brand new password" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // A weak password or a bad email changes nothing, and values stay out of the audit log.
    let (status, _) = call(
        &w.app,
        "PUT",
        &format!("/api/v1/admin/users/{id}"),
        Some(&admin),
        Some(json!({ "bio": "changed", "new_password": "short" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, log) = call(
        &w.app,
        "GET",
        "/api/v1/admin/monitor/audit?action=user_update",
        Some(&admin),
        None,
    )
    .await;
    let detail = log["entries"][0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("password_reset=changed") && !detail.contains("brand new"),
        "{detail}"
    );
}
