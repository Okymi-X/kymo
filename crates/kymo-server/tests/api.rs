use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";

async fn app() -> Router {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    router(AppState::new(pool, PASSWORD).unwrap())
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

async fn login(app: &Router) -> String {
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

#[tokio::test]
async fn admin_routes_require_a_token() {
    let app = app().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/tournaments",
        None,
        Some(json!({"name": "x", "format": "single_elimination"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "password": "wrong" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn full_single_elimination_flow() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());

    let (status, created) = call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Spring Cup 26", "format": "single_elimination"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["slug"], "spring-cup-26");
    let id = created["id"].as_i64().unwrap();

    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/admin/tournaments/{id}/teams"),
        t,
        Some(json!({"names": ["a", "A"]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/admin/tournaments/{id}/teams"),
        t,
        Some(json!({"names": ["a", "b", "c", "d"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, started) = call(
        &app,
        "POST",
        &format!("/api/v1/admin/tournaments/{id}/start"),
        t,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(started["status"], "live");
    assert_eq!(started["bracket"]["matches"].as_array().unwrap().len(), 3);

    let result = |m: usize| format!("/api/v1/admin/tournaments/{id}/matches/{m}/result");
    let (status, _) = call(
        &app,
        "PUT",
        &result(2),
        t,
        Some(json!({"score_a": 1, "score_b": 0})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = call(
        &app,
        "PUT",
        &result(0),
        t,
        Some(json!({"score_a": 2, "score_b": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    for m in [0, 1, 2] {
        let (status, _) = call(
            &app,
            "PUT",
            &result(m),
            t,
            Some(json!({"score_a": 3, "score_b": 1})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, public) = call(&app, "GET", "/api/v1/tournaments/spring-cup-26", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public["status"], "done");

    let (status, undone) = call(&app, "DELETE", &result(2), t, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undone["status"], "live");

    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/admin/tournaments/{id}/teams"),
        t,
        Some(json!({"names": ["x", "y"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn duplicate_tournament_names_conflict_and_unknown_slug_is_404() {
    let app = app().await;
    let token = login(&app).await;
    let body = json!({"name": "Cup", "format": "double_elimination"});
    let (first, _) = call(
        &app,
        "POST",
        "/api/v1/tournaments",
        Some(&token),
        Some(body.clone()),
    )
    .await;
    let (second, _) = call(
        &app,
        "POST",
        "/api/v1/tournaments",
        Some(&token),
        Some(body),
    )
    .await;
    assert_eq!((first, second), (StatusCode::CREATED, StatusCode::CONFLICT));
    let (status, _) = call(&app, "GET", "/api/v1/tournaments/nope", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn repeated_bad_passwords_are_throttled() {
    let app = app().await;
    for _ in 0..5 {
        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({ "password": "nope" })),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "password": PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
