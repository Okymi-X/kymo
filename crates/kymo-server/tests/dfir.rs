//! DFIR challenges in a Jeopardy event: partial credit on the scoreboard, one result per
//! question, and a bundle that keeps the questions.

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

#[tokio::test]
async fn dfir_questions_score_on_a_jeopardy_board() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (alice, alice_id) = player(&app, "alice").await;
    let (status, e) = call(&app, "POST", "/admin/events", Some(&admin), Some(json!({ "name": "Forensics", "kind": "jeopardy", "mode": "solo", "scoreboard_public": true }))).await;
    assert_eq!(status, StatusCode::CREATED, "{e}");
    let (id, slug) = (
        e["id"].as_i64().unwrap(),
        e["slug"].as_str().unwrap().to_owned(),
    );
    let (_, cat) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/categories"),
        Some(&admin),
        Some(json!({ "name": "DFIR" })),
    )
    .await;
    let (status, ch) = call(
        &app,
        "POST",
        &format!("/admin/events/{id}/challenges"),
        Some(&admin),
        Some(json!({ "category_id": cat["id"], "title": "Disk image", "kind": "dfir", "scoring": { "kind": "static", "points": 30 } })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{ch}");
    let cid = ch["id"].as_i64().unwrap();
    // Dynamic points are refused for DFIR everywhere.
    let (status, _) = call(&app, "POST", &format!("/admin/events/{id}/challenges"), Some(&admin), Some(json!({ "category_id": cat["id"], "title": "Bad", "kind": "dfir", "scoring": { "kind": "dynamic", "initial": 100, "minimum": 10, "decay": 5, "curve": "linear" } }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, items) = call(
        &app,
        "PUT",
        &format!("/admin/challenges/{cid}/items"),
        Some(&admin),
        Some(json!({ "items": [
            { "question": "Which user?", "answer": { "kind": "static", "value": "mallory" }, "points": 10 },
            { "question": "Which hash?", "answer": { "kind": "static", "value": "abc123" }, "points": 20, "max_attempts": 2 },
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{items}");
    let (q1, q2) = (
        items[0]["id"].as_i64().unwrap(),
        items[1]["id"].as_i64().unwrap(),
    );
    call(
        &app,
        "POST",
        &format!("/admin/challenges/{cid}/publish"),
        Some(&admin),
        None,
    )
    .await;
    call(
        &app,
        "POST",
        &format!("/admin/events/{id}/participants"),
        Some(&admin),
        Some(json!({ "user_id": alice_id })),
    )
    .await;
    sqlx::query("UPDATE event SET state = 'live' WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    // The player sees the questions, never the answers.
    let (_, board) = call(
        &app,
        "GET",
        &format!("/events/{slug}/challenges"),
        Some(&alice),
        None,
    )
    .await;
    let c = &board["challenges"][0];
    assert_eq!(
        (c["kind"].as_str(), c["items"].as_array().map(Vec::len)),
        (Some("dfir"), Some(2))
    );
    assert!(!board.to_string().contains("mallory"));
    let answer = |item: i64, value: &str| {
        let app = app.clone();
        let (alice, slug, value) = (alice.clone(), slug.clone(), value.to_owned());
        async move {
            call(
                &app,
                "POST",
                &format!("/events/{slug}/challenges/{cid}/items/{item}/submit"),
                Some(&alice),
                Some(json!({ "flag": value })),
            )
            .await
        }
    };
    // The plain flag route is for flag challenges only.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/events/{slug}/challenges/{cid}/submit"),
        Some(&alice),
        Some(json!({ "flag": "x" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, r) = answer(q1, "mallory").await;
    assert_eq!((status, r["points"].as_i64()), (StatusCode::OK, Some(10)));
    let (status, _) = answer(q1, "mallory").await;
    assert_eq!(status, StatusCode::CONFLICT, "a question is answered once");
    let (_, r) = answer(q2, "nope").await;
    assert_eq!(
        (r["correct"].as_bool(), r["attempts_remaining"].as_u64()),
        (Some(false), Some(1))
    );

    // Partial credit is on the scoreboard straight away.
    let (_, sb) = call(
        &app,
        "GET",
        &format!("/events/{slug}/scoreboard"),
        None,
        None,
    )
    .await;
    assert_eq!(sb["rows"][0]["points"], 10);
    let (_, r) = answer(q2, "abc123").await;
    assert_eq!(r["points"], 20);
    let (_, sb) = call(
        &app,
        "GET",
        &format!("/events/{slug}/scoreboard"),
        None,
        None,
    )
    .await;
    assert_eq!(sb["rows"][0]["points"], 30);
    let (_, board) = call(
        &app,
        "GET",
        &format!("/events/{slug}/challenges"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(board["challenges"][0]["solved"], true);

    // A bundle keeps the questions.
    let (status, bundle) = call(
        &app,
        "GET",
        &format!("/admin/events/{id}/export"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bundle["challenges"][0]["kind"], "dfir");
    assert_eq!(
        bundle["challenges"][0]["items"].as_array().unwrap().len(),
        2
    );
    let (status, copy) = call(
        &app,
        "POST",
        "/admin/events/import",
        Some(&admin),
        Some(bundle),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{copy}");
    let (_, copied) = call(
        &app,
        "GET",
        &format!("/admin/events/{}/challenges", copy["id"]),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(copied[0]["kind"], "dfir");
    let (_, copied_items) = call(
        &app,
        "GET",
        &format!("/admin/challenges/{}/items", copied[0]["id"]),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(copied_items.as_array().unwrap().len(), 2);
}
