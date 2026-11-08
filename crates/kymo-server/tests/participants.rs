//! Organizers adding and removing participants of a Jeopardy event, solo and team.

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

async fn event(app: &Router, admin: &str, mode: &str) -> (i64, String) {
    let (status, e) = call(
        app,
        "POST",
        "/admin/events",
        Some(admin),
        Some(json!({ "name": format!("{mode} Cup"), "kind": "jeopardy", "mode": mode })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{e}");
    (
        e["id"].as_i64().unwrap(),
        e["slug"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn organizers_manage_solo_jeopardy_participants_at_any_time() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (_, alice_id) = player(&app, "alice").await;
    let (id, slug) = event(&app, &admin, "solo").await;
    // A draft event takes no sign-ups from players, but an organizer can still add someone.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/events/{slug}/register"),
        Some(&player(&app, "bob").await.0),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (_, found) = call(
        &app,
        "GET",
        &format!("/admin/events/{id}/candidates?q=ali"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(found[0]["id"], alice_id);
    let (status, rows) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&admin),
        Some(json!({ "user_id": alice_id })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{rows}");
    assert_eq!(rows.as_array().unwrap().len(), 1);
    let (status, _) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&admin),
        Some(json!({ "user_id": alice_id })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "no duplicates");
    let (status, _) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&admin),
        Some(json!({ "team_id": 1 })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a solo event takes a player, not a team"
    );
    // Already registered people no longer show up as candidates.
    let (_, found) = call(
        &app,
        "GET",
        &format!("/admin/events/{id}/candidates?q=ali"),
        Some(&admin),
        None,
    )
    .await;
    assert!(found.as_array().unwrap().is_empty());

    // Players cannot do this, and the audit log says who did.
    let (plain, _) = player(&app, "carol").await;
    let (status, _) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&plain),
        Some(json!({ "user_id": alice_id })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, log) = call(
        &app,
        "GET",
        "/admin/monitor/audit?action=participant_added",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(log["total"], 1);

    let participant = rows[0]["id"].as_i64().unwrap();
    let (status, rows) = call(
        &app,
        "DELETE",
        &format!("/admin/events/{id}/participants/{participant}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(rows.as_array().unwrap().is_empty());

    // An ended event's participants are history.
    sqlx::query("UPDATE event SET state = 'ended' WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&admin),
        Some(json!({ "user_id": alice_id })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn organizers_add_existing_teams_to_a_team_jeopardy_event() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (owner, _) = player(&app, "owner").await;
    let (status, team) = call(
        &app,
        "POST",
        "/teams",
        Some(&owner),
        Some(json!({ "name": "Red" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let team_id = team["id"].as_i64().unwrap();
    let (id, _) = event(&app, &admin, "team").await;
    // Teams of one member are fine at the default size rules (1 to 4).
    let (status, rows) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&admin),
        Some(json!({ "team_id": team_id })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{rows}");
    assert_eq!(rows[0]["name"], "Red");
    // The player's own team membership is untouched.
    let (_, mine) = call(&app, "GET", "/teams/mine", Some(&owner), None).await;
    assert_eq!(mine["id"], team_id);
    // A size the event does not allow is refused with the reason.
    sqlx::query("UPDATE event SET team_min = 3 WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, other) = player(&app, "solo").await;
    let _ = other;
    let (owner2, _) = player(&app, "owner2").await;
    let (_, team2) = call(
        &app,
        "POST",
        "/teams",
        Some(&owner2),
        Some(json!({ "name": "Blue" })),
    )
    .await;
    let (status, err) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&admin),
        Some(json!({ "team_id": team2["id"] })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(err["error"].as_str().unwrap().contains("3 to"), "{err}");
}
