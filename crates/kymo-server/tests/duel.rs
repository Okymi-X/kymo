//! Tournament matches decided by challenges: assignment, answers, partial DFIR credit, one
//! result per side and the bracket result.

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
async fn challenges_decide_a_solo_match() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = app_over(&pool);
    let admin = admin_token(&app).await;
    let (alice, _) = player(&app, "alice").await;
    let (bob, _) = player(&app, "bob").await;

    let (status, t) = call(
        &app,
        "POST",
        "/tournaments",
        Some(&admin),
        Some(json!({ "name": "Duel Cup", "format": "single_elimination" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{t}");
    let (tid, event_id, slug) = (
        t["id"].as_i64().unwrap(),
        t["event_id"].as_i64().unwrap(),
        t["slug"].as_str().unwrap().to_owned(),
    );
    sqlx::query("UPDATE event SET mode = 'solo', state = 'registration_open' WHERE id = ?")
        .bind(event_id)
        .execute(&pool)
        .await
        .unwrap();
    for token in [&alice, &bob] {
        let (status, body) = call(
            &app,
            "POST",
            &format!("/events/{slug}/register"),
            Some(token),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    // Two fixed-point challenges: a flag, and a DFIR challenge with two questions.
    let (_, cat) = call(
        &app,
        "POST",
        &format!("/admin/events/{event_id}/categories"),
        Some(&admin),
        Some(json!({ "name": "Forensics" })),
    )
    .await;
    let cat_id = cat["id"].as_i64().unwrap();
    let make = |title: &str, kind: &str, points: i64, flag: Option<&str>| {
        let mut body = json!({
            "category_id": cat_id, "title": title, "kind": kind,
            "scoring": { "kind": "static", "points": points },
        });
        if let Some(f) = flag {
            body["flag"] = json!({ "kind": "static", "value": f });
        }
        body
    };
    let (status, flag_ch) = call(
        &app,
        "POST",
        &format!("/admin/events/{event_id}/challenges"),
        Some(&admin),
        Some(make("Warmup", "flag", 10, Some("alpha"))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{flag_ch}");
    let (status, dfir) = call(
        &app,
        "POST",
        &format!("/admin/events/{event_id}/challenges"),
        Some(&admin),
        Some(make("Disk image", "dfir", 30, None)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{dfir}");
    let (flag_id, dfir_id) = (
        flag_ch["id"].as_i64().unwrap(),
        dfir["id"].as_i64().unwrap(),
    );

    // Dynamic points are refused for tournament challenges, and the questions must add up.
    let mut dynamic = make("Dynamic", "flag", 10, Some("x"));
    dynamic["scoring"] =
        json!({ "kind": "dynamic", "initial": 100, "minimum": 10, "decay": 5, "curve": "linear" });
    let (status, _) = call(
        &app,
        "POST",
        &format!("/admin/events/{event_id}/challenges"),
        Some(&admin),
        Some(dynamic),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let items = |second: i64| {
        json!({ "items": [
            { "question": "Which user?", "answer": { "kind": "static", "value": "mallory" }, "points": 10 },
            { "question": "Which hash?", "answer": { "kind": "static", "value": "abc123" }, "points": second },
        ]})
    };
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/admin/challenges/{dfir_id}/items"),
        Some(&admin),
        Some(items(5)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "items that do not add up to 30 are refused"
    );
    let (status, saved) = call(
        &app,
        "PUT",
        &format!("/admin/challenges/{dfir_id}/items"),
        Some(&admin),
        Some(items(20)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let item_ids: Vec<i64> = saved
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_i64().unwrap())
        .collect();
    for id in [flag_id, dfir_id] {
        let (status, body) = call(
            &app,
            "POST",
            &format!("/admin/challenges/{id}/publish"),
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    // Settings, roster from registrations, start.
    let (status, body) = call(
        &app,
        "PUT",
        &format!("/admin/tournaments/{tid}/challenge-settings"),
        Some(&admin),
        Some(json!({ "assignment": "per_match", "solo": 2, "team": 3, "seed": 7, "time_limit": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, roster) = call(
        &app,
        "POST",
        &format!("/admin/tournaments/{tid}/roster/from-registrations"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{roster}");
    assert_eq!(roster["teams"].as_array().unwrap().len(), 2);
    assert!(roster["teams"][0]["user_id"].is_i64());
    let (status, started) = call(
        &app,
        "POST",
        &format!("/admin/tournaments/{tid}/start"),
        Some(&admin),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert!(
        started["teams"][0]["user_id"].is_i64(),
        "links survive the start"
    );

    // The players see their match, with both challenges drawn.
    let (_, mine) = call(
        &app,
        "GET",
        &format!("/tournaments/{slug}/play/mine"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(mine["matches"], json!([0]));
    let (status, view) = call(
        &app,
        "GET",
        &format!("/tournaments/{slug}/matches/0/play"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["status"], "open");
    assert_eq!(view["challenges"].as_array().unwrap().len(), 2);
    // Questions are shown, answers never.
    assert!(!view.to_string().contains("mallory"));

    let submit = |token: &str, challenge: i64, value: &str, item: Option<i64>| {
        let uri = format!("/tournaments/{slug}/matches/0/challenges/{challenge}/submit");
        let body = json!({ "value": value, "item_id": item });
        (token.to_owned(), uri, body)
    };
    let send = |req: (String, String, Value)| {
        let app = app.clone();
        async move { call(&app, "POST", &req.1, Some(&req.0), Some(req.2)).await }
    };
    let (status, r) = send(submit(&alice, flag_id, "alpha", None)).await;
    assert_eq!(status, StatusCode::OK, "{r}");
    assert_eq!(
        (r["correct"].as_bool(), r["points"].as_u64()),
        (Some(true), Some(10))
    );
    let (status, _) = send(submit(&alice, flag_id, "alpha", None)).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a second correct answer never scores twice"
    );
    let (_, r) = send(submit(&alice, dfir_id, "mallory", Some(item_ids[0]))).await;
    assert_eq!(r["points"], 10);
    let (_, r) = send(submit(&alice, dfir_id, "wrong", Some(item_ids[1]))).await;
    assert_eq!(r["correct"], false);
    // A question that is not part of the challenge is refused.
    let (status, _) = send(submit(&alice, dfir_id, "x", Some(999_999))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Someone who is not playing cannot answer or look.
    let (carol, _) = player(&app, "carol").await;
    let (status, _) = call(
        &app,
        "GET",
        &format!("/tournaments/{slug}/matches/0/play"),
        Some(&carol),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Bob answers one flag question wrongly; the match is not over yet.
    let (_, r) = send(submit(&bob, flag_id, "nope", None)).await;
    assert_eq!(r["correct"], false);
    let (_, t) = call(&app, "GET", &format!("/tournaments/{slug}"), None, None).await;
    assert!(t["bracket"]["matches"][0]["score"].is_null());

    // An organizer closes it with what was earned: 20 to 0, alice advances, the series is won.
    let (status, done) = call(
        &app,
        "POST",
        &format!("/admin/tournaments/{tid}/matches/0/finalize"),
        Some(&admin),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["status"], "final");
    let (_, t) = call(&app, "GET", &format!("/tournaments/{slug}"), None, None).await;
    assert_eq!(t["bracket"]["matches"][0]["score"], json!([20, 0]));
    assert_eq!(t["bracket"]["matches"][0]["winner"], "A");
    assert_eq!(t["status"], "done");
    let (status, _) = send(submit(&bob, flag_id, "alpha", None)).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a decided match takes no more answers"
    );

    // Every judged answer is kept, with who sent it.
    let (_, staff) = call(
        &app,
        "GET",
        &format!("/admin/tournaments/{tid}/matches/0/duel"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(staff["answers"].as_array().unwrap().len(), 4);
}
