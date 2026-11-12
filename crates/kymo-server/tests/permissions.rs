//! The organizer-editable role and permission matrix: reading it, changing it, what it
//! enforces, what is locked, that it survives a restart and that every change is audited.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";

fn app_over(pool: &SqlitePool) -> Router {
    router(AppState::new(pool.clone(), PASSWORD).unwrap())
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(format!("/api/v1{uri}"));
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
        "/auth/login",
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
        "/auth/register",
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

async fn set_role(app: &Router, admin: &str, user: i64, role: &str) {
    let (status, _) = call(
        app,
        "PUT",
        &format!("/admin/users/{user}"),
        Some(admin),
        Some(json!({ "role": role })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

fn granted(matrix: &Value, role: &str) -> Vec<String> {
    matrix["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["role"] == role)
        .unwrap()["granted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect()
}

async fn audit(app: &Router, admin: &str, action: &str) -> Value {
    let (status, body) = call(
        app,
        "GET",
        &format!("/admin/monitor/audit?action={action}"),
        Some(admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body
}

#[tokio::test]
async fn the_defaults_are_visible_to_admins_only() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (plain, _) = player(&app, "plain").await;

    let (status, _) = call(&app, "GET", "/admin/permissions", Some(&plain), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(&app, "GET", "/admin/permissions", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, matrix) = call(&app, "GET", "/admin/permissions", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(matrix["permissions"].as_array().unwrap().len(), 17);
    let roles: Vec<&str> = matrix["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["visitor", "player", "referee", "monitor", "admin"]);
    assert_eq!(granted(&matrix, "admin").len(), 17);
    assert!(granted(&matrix, "player").contains(&"submit_flag".to_owned()));
    assert!(!granted(&matrix, "player").contains(&"manage_events".to_owned()));
    let locked: Vec<bool> = matrix["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["locked"].as_bool().unwrap())
        .collect();
    assert_eq!(locked, [true, false, false, false, true]);
}

#[tokio::test]
async fn a_caller_sees_what_it_may_do() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (plain, _) = player(&app, "plain").await;

    let (_, visitor) = call(&app, "GET", "/permissions/mine", None, None).await;
    assert_eq!(visitor["role"], "visitor");
    assert_eq!(visitor["permissions"], json!(["view_public"]));

    let (_, mine) = call(&app, "GET", "/permissions/mine", Some(&plain), None).await;
    assert_eq!(mine["role"], "player");
    assert!(
        mine["permissions"]
            .as_array()
            .unwrap()
            .contains(&json!("submit_flag"))
    );

    let (_, org) = call(&app, "GET", "/permissions/mine", Some(&admin), None).await;
    assert_eq!(org["role"], "admin");
    assert_eq!(org["permissions"].as_array().unwrap().len(), 17);

    // A stale token is still a 401, not a visitor.
    let (status, _) = call(&app, "GET", "/permissions/mine", Some("nope"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn turning_a_player_permission_off_blocks_it_and_turning_it_on_restores_it() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (alice, _) = player(&app, "alice").await;
    let (bob, _) = player(&app, "bob").await;

    let (status, _) = call(
        &app,
        "POST",
        "/teams",
        Some(&alice),
        Some(json!({"name": "Red"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Players may no longer manage teams.
    let player_without_teams = json!({ "roles": { "player": [
        "view_public", "view_scoreboard_public", "play_event", "submit_flag", "start_instance"
    ]}});
    let (status, matrix) = call(
        &app,
        "PUT",
        "/admin/permissions",
        Some(&admin),
        Some(player_without_teams),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{matrix}");
    assert!(!granted(&matrix, "player").contains(&"manage_team".to_owned()));

    // Takes effect at once, for accounts that already exist and their live sessions.
    let (status, body) = call(
        &app,
        "POST",
        "/teams",
        Some(&bob),
        Some(json!({"name": "Blue"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, _) = call(&app, "GET", "/teams/mine", Some(&alice), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Other player abilities are untouched.
    let (status, _) = call(&app, "GET", "/me", Some(&bob), None).await;
    assert_eq!(status, StatusCode::OK);

    // Back on.
    let (status, _) = call(&app, "POST", "/admin/permissions/reset", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &app,
        "POST",
        "/teams",
        Some(&bob),
        Some(json!({"name": "Blue"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn an_admin_can_hand_a_role_extra_powers() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (judge, judge_id) = player(&app, "judge").await;
    set_role(&app, &admin, judge_id, "monitor").await;

    let (status, _) = call(&app, "GET", "/admin/events", Some(&judge), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(&app, "GET", "/admin/permissions", Some(&judge), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = call(
        &app,
        "PUT",
        "/admin/permissions",
        Some(&admin),
        Some(json!({ "roles": { "monitor": [
            "view_public", "monitor", "moderate_bans", "view_audit", "manage_events"
        ]}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&app, "GET", "/admin/events", Some(&judge), None).await;
    assert_eq!(status, StatusCode::OK);
    // Still no user management, which was not granted.
    let (status, _) = call(&app, "GET", "/admin/users", Some(&judge), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Granting the permission to edit permissions makes that role an editor too.
    let (status, _) = call(
        &app,
        "PUT",
        "/admin/permissions",
        Some(&admin),
        Some(json!({ "roles": { "monitor": [
            "view_public", "monitor", "manage_permissions"
        ]}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, matrix) = call(&app, "GET", "/admin/permissions", Some(&judge), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        granted(&matrix, "monitor"),
        ["view_public", "monitor", "manage_permissions"]
    );
    // The editor also lost manage_events in that change, at once.
    let (status, _) = call(&app, "GET", "/admin/events", Some(&judge), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn nobody_can_lock_the_admin_out_or_hide_the_public_pages() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;

    for body in [
        json!({ "roles": { "admin": ["view_public"] }}),
        json!({ "roles": { "visitor": ["view_public", "manage_users"] }}),
        json!({ "roles": { "player": ["play_event"] }}),
    ] {
        let (status, err) = call(&app, "PUT", "/admin/permissions", Some(&admin), Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{err}");
        assert!(err["error"].as_str().is_some_and(|m| !m.is_empty()));
    }
    // Unknown roles and permissions are refused, not ignored.
    for body in [
        json!({ "roles": { "root": ["view_public"] }}),
        json!({ "roles": { "player": ["view_public", "fly"] }}),
    ] {
        let (status, _) = call(&app, "PUT", "/admin/permissions", Some(&admin), Some(body)).await;
        assert!(status.is_client_error(), "{status}");
    }
    // A request with one bad part changes nothing, even the good parts.
    let (status, _) = call(
        &app,
        "PUT",
        "/admin/permissions",
        Some(&admin),
        Some(json!({ "roles": {
            "monitor": ["view_public", "monitor", "manage_users"],
            "admin": []
        }})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, matrix) = call(&app, "GET", "/admin/permissions", Some(&admin), None).await;
    assert!(!granted(&matrix, "monitor").contains(&"manage_users".to_owned()));
    assert_eq!(granted(&matrix, "admin").len(), 17);
}

#[tokio::test]
async fn the_configuration_survives_a_restart() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    {
        let app = app_over(&pool);
        let admin = admin_token(&app).await;
        let (status, _) = call(
            &app,
            "PUT",
            "/admin/permissions",
            Some(&admin),
            Some(json!({ "roles": { "referee": ["view_public", "referee"] }})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    // A new server over the same database.
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (_, matrix) = call(&app, "GET", "/admin/permissions", Some(&admin), None).await;
    assert_eq!(granted(&matrix, "referee"), ["view_public", "referee"]);

    // Only differences from the defaults are stored, so the row is small and a permission
    // added in a later version still follows its default.
    let stored: String =
        sqlx::query_scalar("SELECT value FROM setting WHERE key = 'role_permissions'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let rows: Vec<Value> = serde_json::from_str(&stored).unwrap();
    assert!(
        rows.iter()
            .all(|r| r["role"] == "referee" && r["allowed"] == false)
    );
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn a_damaged_or_foreign_row_falls_back_to_the_defaults() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    // One unknown permission and one entry that tries to strip the admin: both ignored.
    sqlx::query("INSERT INTO setting (key, value) VALUES ('role_permissions', ?)")
        .bind(
            r#"[{"role":"player","permission":"time_travel","allowed":true},
                  {"role":"admin","permission":"manage_settings","allowed":false},
                  {"role":"monitor","permission":"manage_events","allowed":true}]"#,
        )
        .execute(&pool)
        .await
        .unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (status, matrix) = call(&app, "GET", "/admin/permissions", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(granted(&matrix, "admin").len(), 17);
    assert!(granted(&matrix, "monitor").contains(&"manage_events".to_owned()));

    sqlx::query("UPDATE setting SET value = 'not json' WHERE key = 'role_permissions'")
        .execute(&pool)
        .await
        .unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (status, matrix) = call(&app, "GET", "/admin/permissions", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!granted(&matrix, "monitor").contains(&"manage_events".to_owned()));
}

#[tokio::test]
async fn every_change_is_in_the_audit_log() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;

    // A change that changes nothing writes nothing.
    let (status, _) = call(
        &app,
        "PUT",
        "/admin/permissions",
        Some(&admin),
        Some(json!({ "roles": { "monitor": ["view_public", "monitor", "moderate_bans", "view_audit"] }})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(audit(&app, &admin, "permissions_updated").await["total"], 0);

    let (status, _) = call(
        &app,
        "PUT",
        "/admin/permissions",
        Some(&admin),
        Some(json!({ "roles": { "monitor": ["view_public", "monitor", "manage_events"] }})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let log = audit(&app, &admin, "permissions_updated").await;
    assert_eq!(log["total"], 1);
    let entry = &log["entries"][0];
    assert_eq!(entry["target_kind"], "role");
    assert_eq!(entry["target_id"], "monitor");
    assert_eq!(entry["actor_role"], "admin");
    assert_eq!(
        entry["detail"],
        "granted: manage_events; revoked: moderate_bans, view_audit"
    );

    let (status, _) = call(&app, "POST", "/admin/permissions/reset", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK);
    let log = audit(&app, &admin, "permissions_reset").await;
    assert_eq!(log["total"], 1);
    assert_eq!(
        log["entries"][0]["detail"],
        "granted: moderate_bans, view_audit; revoked: manage_events"
    );
    // Resetting defaults again changes nothing and logs nothing.
    call(&app, "POST", "/admin/permissions/reset", Some(&admin), None).await;
    assert_eq!(audit(&app, &admin, "permissions_reset").await["total"], 1);
}
