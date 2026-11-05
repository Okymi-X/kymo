//! The sample data: tournaments and Jeopardy events, safe to run twice.

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use http_body_util::BodyExt;
use kymo_server::{AppState, db, demo, router};
use serde_json::Value;
use tower::ServiceExt;

async fn get(app: &axum::Router, uri: &str) -> (u16, Value) {
    let addr: SocketAddr = "10.0.0.1:4000".parse().unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1{uri}"))
                .extension(ConnectInfo(addr))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn seeding_adds_tournaments_and_jeopardy_events_once() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let first = demo::seed(&pool).await.unwrap();
    assert!(
        first >= 11,
        "eight tournaments and three Jeopardy events, got {first}"
    );
    assert_eq!(
        demo::seed(&pool).await.unwrap(),
        0,
        "running it again adds nothing"
    );

    let app = router(AppState::new(pool, "correct horse battery").unwrap());
    let (_, events) = get(&app, "/events").await;
    let kinds: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"jeopardy") && kinds.contains(&"tournament"),
        "{kinds:?}"
    );

    // The live event has challenges in categories and a scoreboard with real scores.
    let (s, board) = get(&app, "/events/spring-jeopardy-ctf/scoreboard").await;
    assert_eq!(s, 200, "{board}");
    let rows = board["rows"]
        .as_array()
        .or(board["entries"].as_array())
        .expect("rows");
    assert_eq!(rows.len(), 14);
    assert!(rows[0]["points"].as_i64().unwrap() > rows[13]["points"].as_i64().unwrap_or(0));

    // The finished event is ended and the upcoming one has nobody solving yet.
    let (_, winter) = get(&app, "/events/winter-jeopardy").await;
    assert_eq!(winter["effective_state"], "ended");
    let (_, summer) = get(&app, "/events/summer-jeopardy").await;
    assert_eq!(summer["state"], "registration_open");
}
