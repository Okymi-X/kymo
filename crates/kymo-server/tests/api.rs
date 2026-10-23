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
        Some(json!({"teams": [{"name": "a"}, {"name": "A"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/v1/admin/tournaments/{id}/teams"),
        t,
        Some(json!({"teams": [{"name": "a"}, {"name": "b"}, {"name": "c"}, {"name": "d"}]})),
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
        Some(json!({"teams": [{"name": "x"}, {"name": "y"}]})),
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

#[tokio::test]
async fn score_seeding_orders_teams_by_points() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Qualifier", "format": "single_elimination"})),
    )
    .await;
    let teams = json!({"teams": [{"name": "low", "score": 10}, {"name": "top", "score": 900}, {"name": "mid", "score": 400}]});
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(teams),
    )
    .await;
    let (status, started) = call(
        &app,
        "POST",
        "/api/v1/admin/tournaments/1/start",
        t,
        Some(json!({"seeding": "score"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order: Vec<_> = started["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["top", "mid", "low"]);
}

#[tokio::test]
async fn api_tokens_authenticate_until_revoked_and_sessions_survive_restart() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = router(AppState::new(pool.clone(), PASSWORD).unwrap());
    let session = login(&app).await;

    let (status, made) = call(
        &app,
        "POST",
        "/api/v1/admin/tokens",
        Some(&session),
        Some(json!({"label": "ci"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let secret = made["token"].as_str().unwrap().to_owned();

    let new_tournament = json!({"name": "Via token", "format": "single_elimination"});
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/tournaments",
        Some(&secret),
        Some(new_tournament),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, listed) = call(&app, "GET", "/api/v1/admin/tokens", Some(&session), None).await;
    assert!(listed[0].get("token").is_none());
    let id = listed[0]["id"].as_i64().unwrap();
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/v1/admin/tokens/{id}"),
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&app, "GET", "/api/v1/admin/tokens", Some(&secret), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let restarted = router(AppState::new(pool, PASSWORD).unwrap());
    let (status, _) = call(
        &restarted,
        "GET",
        "/api/v1/admin/tokens",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// Plays every ready match (side A wins) through the HTTP API until the tournament is done.
async fn play_out(app: &Router, token: &str, id: i64) -> Value {
    for _ in 0..64 {
        let (_, t) = call(app, "GET", "/api/v1/tournaments/play-off", None, None).await;
        if t["status"] == "done" {
            return t;
        }
        let ready = t["bracket"]["matches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| {
                m["score"].is_null()
                    && m["slots"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|s| s["kind"] == "team")
            })
            .map(|m| m["id"].as_u64().unwrap());
        let Some(match_id) = ready else { break };
        let uri = format!("/api/v1/admin/tournaments/{id}/matches/{match_id}/result");
        let (status, _) = call(
            app,
            "PUT",
            &uri,
            Some(token),
            Some(json!({"score_a": 2, "score_b": 1})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    panic!("tournament never finished");
}

#[tokio::test]
async fn double_elimination_plays_to_a_champion_over_the_api() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Play Off", "format": "double_elimination"})),
    )
    .await;
    let roster: Vec<_> = (1..=7)
        .map(|n| json!({"name": format!("team {n}"), "score": 100 - n}))
        .collect();
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": roster})),
    )
    .await;
    call(
        &app,
        "POST",
        "/api/v1/admin/tournaments/1/start",
        t,
        Some(json!({"seeding": "score"})),
    )
    .await;

    let done = play_out(&app, &token, 1).await;
    assert_eq!(done["status"], "done");
    let champion = done["champion"].as_i64().unwrap();
    let top_seed = done["teams"][0]["id"].as_i64().unwrap();
    assert_eq!(champion, top_seed);
}

#[tokio::test]
async fn forfeit_over_the_api_advances_a_team_and_validates_the_side() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Walkover", "format": "single_elimination"})),
    )
    .await;
    let teams = json!({"teams": [{"name": "a"}, {"name": "b"}, {"name": "c"}, {"name": "d"}]});
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(teams),
    )
    .await;
    call(&app, "POST", "/api/v1/admin/tournaments/1/start", t, None).await;

    let uri = "/api/v1/admin/tournaments/1/matches/0/forfeit";
    let (status, _) = call(&app, "PUT", uri, t, Some(json!({"winner": "C"}))).await;
    assert!(status.is_client_error());
    let (status, out) = call(&app, "PUT", uri, t, Some(json!({"winner": "B"}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["bracket"]["matches"][0]["forfeit"], true);
    assert_eq!(out["bracket"]["matches"][2]["slots"][0]["kind"], "team");
}

#[tokio::test]
async fn third_place_option_adds_a_match_and_keeps_the_champion() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    let body = json!({"name": "Bronze", "format": "single_elimination", "third_place": true});
    let (status, created) = call(&app, "POST", "/api/v1/tournaments", t, Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["third_place"], true);
    let teams = json!({"teams": [{"name": "a"}, {"name": "b"}, {"name": "c"}, {"name": "d"}]});
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(teams),
    )
    .await;
    let (_, started) = call(&app, "POST", "/api/v1/admin/tournaments/1/start", t, None).await;
    assert_eq!(started["bracket"]["matches"].as_array().unwrap().len(), 4);
    let uri = |m: usize| format!("/api/v1/admin/tournaments/1/matches/{m}/result");
    for m in [0, 1, 2] {
        call(
            &app,
            "PUT",
            &uri(m),
            t,
            Some(json!({"score_a": 2, "score_b": 1})),
        )
        .await;
    }
    let (_, done) = call(&app, "GET", "/api/v1/tournaments/bronze", None, None).await;
    assert_eq!(done["status"], "done");
    let (status, _) = call(
        &app,
        "PUT",
        &uri(3),
        t,
        Some(json!({"score_a": 1, "score_b": 0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn appearance_is_public_editable_by_admins_and_validated() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());

    let (status, defaults) = call(&app, "GET", "/api/v1/settings", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(defaults["site_name"], "Kymo");

    let custom = json!({"site_name": " HTB Club ", "accent_light": "#aa2200", "accent_dark": "#ff8866", "radius": 14, "font": "serif"});
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/admin/settings",
        None,
        Some(custom.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, saved) = call(&app, "PUT", "/api/v1/admin/settings", t, Some(custom)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["site_name"], "HTB Club");

    let (_, public) = call(&app, "GET", "/api/v1/settings", None, None).await;
    assert_eq!(public["accent_light"], "#aa2200");
    assert_eq!(public["font"], "serif");

    for bad in [
        json!({"site_name": "", "accent_light": "#aa2200", "accent_dark": "#ff8866", "radius": 8, "font": "sans"}),
        json!({"site_name": "x", "accent_light": "red", "accent_dark": "#ff8866", "radius": 8, "font": "sans"}),
        json!({"site_name": "x", "accent_light": "#aa2200", "accent_dark": "#ff8866", "radius": 99, "font": "sans"}),
    ] {
        let (status, _) = call(&app, "PUT", "/api/v1/admin/settings", t, Some(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    let (status, reset) = call(&app, "DELETE", "/api/v1/admin/settings", t, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reset["accent_light"], "#0a6fe0");
    let (_, after) = call(&app, "GET", "/api/v1/settings", None, None).await;
    assert_eq!(after["site_name"], "Kymo");
}
