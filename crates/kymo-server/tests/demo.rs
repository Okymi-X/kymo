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

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (u16, Value) {
    let addr: SocketAddr = "10.0.0.1:4000".parse().unwrap();
    let mut req = Request::builder()
        .method(method)
        .uri(format!("/api/v1{uri}"))
        .extension(ConnectInfo(addr));
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
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn login(app: &axum::Router, user: &str, password: &str) -> String {
    let (status, body) = send(
        app,
        "POST",
        "/auth/player-login",
        None,
        Some(serde_json::json!({ "username": user, "password": password })),
    )
    .await;
    assert_eq!(status, 200, "{user}: {body}");
    body["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn the_platform_seed_fills_every_feature_and_hands_out_one_password() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let state = AppState::new(pool, "correct horse battery").unwrap();
    let report = kymo_server::demo_platform::seed_all(&state).await.unwrap();
    let app = router(state.clone());

    // One generated password signs in every kind of account.
    assert_eq!(report.password.len(), 16);
    let admin = login(&app, "demo-admin", &report.password).await;
    let ada = login(&app, "demo-ada", &report.password).await;
    login(&app, "demo-referee", &report.password).await;
    login(&app, "demo-newbie", &report.password).await;
    let (_, me) = send(&app, "GET", "/me", Some(&admin), None).await;
    assert_eq!(me["role"], "admin");

    // The lifecycle states are all there.
    let (_, events) = send(&app, "GET", "/events", None, None).await;
    let state_of = |slug: &str| {
        events
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["slug"] == slug)
            .map(|e| e["effective_state"].as_str().unwrap().to_owned())
    };
    assert_eq!(state_of("forensics-weekend").as_deref(), Some("live"));
    assert_eq!(state_of("paused-practice").as_deref(), Some("paused"));
    assert_eq!(state_of("autumn-forensics").as_deref(), Some("ended"));
    let early = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "early-bird-ctf")
        .unwrap();
    assert_eq!(early["started_early"], true);
    let closed = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "closed-registration-cup")
        .unwrap();
    assert_eq!(closed["registration_open"], false);

    // The public board is frozen, staff see the live one, and partial credit counts.
    let (_, public) = send(
        &app,
        "GET",
        "/events/forensics-weekend/scoreboard",
        None,
        None,
    )
    .await;
    let (_, staff) = send(
        &app,
        "GET",
        "/events/forensics-weekend/scoreboard",
        Some(&admin),
        None,
    )
    .await;
    assert!(
        public["frozen_at"].is_i64() && staff["frozen_at"].is_null(),
        "{public} {staff}"
    );
    let points = |board: &Value| {
        board["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["points"].as_i64().unwrap())
            .sum::<i64>()
    };
    assert!(
        points(&staff) >= points(&public),
        "staff see at least what the public sees"
    );
    assert!(public["rows"].as_array().unwrap().len() >= 12);

    // Single flags and multi-question challenges, with partly answered questions.
    let (_, board) = send(
        &app,
        "GET",
        "/events/forensics-weekend/challenges",
        Some(&ada),
        None,
    )
    .await;
    let multi: Vec<&Value> = board["challenges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "dfir")
        .collect();
    assert_eq!(multi.len(), 3);
    assert!(
        multi.iter().any(|c| c["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["solved"] == true)),
        "partial credit exists"
    );
    assert!(
        board["challenges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "flag")
    );

    // Tournaments decided by challenges: a decided match, a tied one broken by time, one in play.
    let (_, duel) = send(&app, "GET", "/tournaments/duel-cup", None, None).await;
    assert_eq!(duel["challenges"]["assignment"], "per_match");
    assert!(
        duel["schedule"]["ends_at"].is_i64(),
        "the tournament counts down to its end"
    );
    assert_eq!(duel["schedule"]["effective_state"], "live");
    let matches = duel["bracket"]["matches"].as_array().unwrap();
    assert!(
        !matches[0]["score"].is_null() && !matches[1]["score"].is_null(),
        "two matches are decided"
    );
    assert_eq!(
        matches[0]["score"][0], matches[0]["score"][1],
        "the first match ended tied"
    );
    assert!(
        !matches[0]["winner"].is_null(),
        "and was broken by who finished first"
    );
    assert!(
        matches[2]["score"].is_null(),
        "the third is still being played"
    );
    let (_, team_duel) = send(&app, "GET", "/tournaments/team-duel-cup", None, None).await;
    assert_eq!(team_duel["challenges"]["assignment"], "per_round");
    assert!(team_duel["teams"][0]["player_team_id"].is_i64());
    let (_, league) = send(&app, "GET", "/tournaments/challenge-league", None, None).await;
    assert_eq!(league["challenges"]["assignment"], "manual");

    // The ladder has a dated history and the audit log shows the organizer's work.
    let (_, ladder) = send(&app, "GET", "/ladder/timeline", None, None).await;
    assert!(!ladder.as_array().unwrap().is_empty());
    let (_, audit) = send(
        &app,
        "GET",
        "/admin/monitor/audit?action=participant_added",
        Some(&admin),
        None,
    )
    .await;
    assert!(audit["total"].as_i64().unwrap() >= 1);

    // Running it again adds nothing and hands out a new password that works.
    let again = kymo_server::demo_platform::seed_all(&state).await.unwrap();
    assert_eq!(again.added, 0);
    assert_ne!(again.password, report.password);
    login(&app, "demo-ada", &again.password).await;
    let (status, _) = send(
        &app,
        "POST",
        "/auth/player-login",
        None,
        Some(serde_json::json!({ "username": "demo-ada", "password": report.password })),
    )
    .await;
    assert_eq!(status, 401, "the old password no longer works");
}
