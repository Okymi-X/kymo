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

#[tokio::test]
async fn list_reports_progress_without_sending_brackets() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Progress", "format": "single_elimination"})),
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
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/matches/0/result",
        t,
        Some(json!({"score_a": 1, "score_b": 0})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/v1/tournaments", None, None).await;
    assert_eq!(list[0]["progress"], json!({"played": 1, "total": 3}));
    assert!(list[0]["bracket"].is_null());
}

async fn raw(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: &str,
) -> (StatusCode, String, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let kind = res
        .headers()
        .get("content-type")
        .map_or(String::new(), |v| v.to_str().unwrap().to_owned());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, kind, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn framework_errors_use_the_json_envelope_without_internals() {
    let app = app().await;
    let token = login(&app).await;
    let cases = [
        ("POST", "/api/v1/tournaments", r#"{"name":"#),
        (
            "POST",
            "/api/v1/tournaments",
            r#"{"name":12,"format":"single_elimination"}"#,
        ),
        ("PATCH", "/api/v1/admin/tokens", "{}"),
        ("GET", "/api/v1/does-not-exist", ""),
    ];
    for (method, uri, body) in cases {
        let (status, kind, text) = raw(&app, method, uri, Some(&token), body).await;
        assert!(status.is_client_error(), "{method} {uri}");
        assert!(
            kind.starts_with("application/json"),
            "{method} {uri} gave {kind}"
        );
        assert!(
            !text.contains("deserialize") && !text.contains("serde"),
            "{text}"
        );
        assert!(serde_json::from_str::<Value>(&text).unwrap()["error"].is_string());
    }
}

#[tokio::test]
async fn throttle_is_per_client_and_sends_retry_after() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let state = AppState::new(pool, PASSWORD).unwrap();
    let (attacker, organizer) = (
        "203.0.113.9".parse().unwrap(),
        "198.51.100.4".parse().unwrap(),
    );
    for _ in 0..5 {
        state.record_login(attacker, false);
    }
    assert!(state.login_blocked(attacker));
    assert!(!state.login_blocked(organizer));
    state.record_login(attacker, true);
    assert!(!state.login_blocked(attacker));

    let app = router(state);
    for _ in 0..5 {
        raw(
            &app,
            "POST",
            "/api/v1/auth/login",
            None,
            r#"{"password":"nope"}"#,
        )
        .await;
    }
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"password":"nope"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.headers().get("retry-after").unwrap(), "60");
}

#[tokio::test]
async fn names_scores_and_options_are_validated() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());

    let (status, made) = call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "東京大会", "format": "double_elimination", "third_place": true})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(made["slug"], "東京大会");
    assert_eq!(made["third_place"], false);
    let (status, _) = call(
        &app,
        "GET",
        &format!(
            "/api/v1/tournaments/{}",
            "%E6%9D%B1%E4%BA%AC%E5%A4%A7%E4%BC%9A"
        ),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for bad in ["a\u{0}b", "x\u{202E}y", "\u{200B}"] {
        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/tournaments",
            t,
            Some(json!({"name": bad, "format": "single_elimination"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": [{"name": "a", "score": 1_000_001}]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_slugs_and_files_return_404_and_logout_ends_the_session() {
    let app = app().await;
    let (status, _, _) = raw(&app, "GET", "/api/v1/tournaments/nope/events", None, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for path in ["/favicon.ico", "/.env", "/assets/missing.js"] {
        let (status, _, _) = raw(&app, "GET", path, None, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (status, _, _) = raw(&app, "GET", "/t/some-route", None, "").await;
    assert_eq!(status, StatusCode::OK);

    let token = login(&app).await;
    let (status, _) = call(&app, "GET", "/api/v1/admin/tokens", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = raw(&app, "POST", "/api/v1/auth/logout", Some(&token), "").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&app, "GET", "/api/v1/admin/tokens", Some(&token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn round_robin_plays_with_draws_to_a_champion_and_caps_the_roster() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    let (status, made) = call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "League", "format": "round_robin", "third_place": true})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(made["third_place"], false);

    let too_many: Vec<_> = (0..33).map(|n| json!({"name": format!("t{n}")})).collect();
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": too_many})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let teams: Vec<_> = (1..=4)
        .map(|n| json!({"name": format!("team {n}")}))
        .collect();
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": teams})),
    )
    .await;
    let (_, started) = call(&app, "POST", "/api/v1/admin/tournaments/1/start", t, None).await;
    assert_eq!(started["bracket"]["matches"].as_array().unwrap().len(), 6);

    let uri = |m: usize| format!("/api/v1/admin/tournaments/1/matches/{m}/result");
    let (status, _) = call(
        &app,
        "PUT",
        &uri(0),
        t,
        Some(json!({"score_a": 1, "score_b": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for m in 1..6 {
        call(
            &app,
            "PUT",
            &uri(m),
            t,
            Some(json!({"score_a": 2, "score_b": 0})),
        )
        .await;
    }
    let (_, done) = call(&app, "GET", "/api/v1/tournaments/league", None, None).await;
    assert_eq!(done["status"], "done");
    assert!(done["champion"].is_number());
    assert_eq!(done["progress"], json!({"played": 6, "total": 6}));

    // Knockout formats still reject a draw.
    call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Knock", "format": "single_elimination"})),
    )
    .await;
    let four: Vec<_> = (1..=4).map(|n| json!({"name": format!("k{n}")})).collect();
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/2/teams",
        t,
        Some(json!({"teams": four})),
    )
    .await;
    call(&app, "POST", "/api/v1/admin/tournaments/2/start", t, None).await;
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/2/matches/0/result",
        t,
        Some(json!({"score_a": 1, "score_b": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn groups_then_knockout_runs_over_the_api_and_validates_its_setup() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());

    let bad = json!({"name": "Bad", "format": "group_knockout", "groups": 9, "advance": 2});
    let (status, _) = call(&app, "POST", "/api/v1/tournaments", t, Some(bad)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let body = json!({"name": "Worlds", "format": "group_knockout", "groups": 2, "advance": 2});
    let (status, made) = call(&app, "POST", "/api/v1/tournaments", t, Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        (made["groups"].as_u64(), made["advance"].as_u64()),
        (Some(2), Some(2))
    );

    let few: Vec<_> = (1..=5).map(|n| json!({"name": format!("f{n}")})).collect();
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": few})),
    )
    .await;
    let (status, error) = call(&app, "POST", "/api/v1/admin/tournaments/1/start", t, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("at least 6"));

    let eight: Vec<_> = (1..=8)
        .map(|n| json!({"name": format!("team {n}"), "score": 100 - n}))
        .collect();
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": eight})),
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
    assert_eq!(started["bracket"]["matches"].as_array().unwrap().len(), 15);

    for _ in 0..40 {
        let (_, tour) = call(&app, "GET", "/api/v1/tournaments/worlds", None, None).await;
        if tour["status"] == "done" {
            assert!(tour["champion"].is_number());
            assert_eq!(tour["progress"]["played"], tour["progress"]["total"]);
            return;
        }
        let next = tour["bracket"]["matches"]
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
        let id = next.expect("a match should be ready until the tournament is done");
        let uri = format!("/api/v1/admin/tournaments/1/matches/{id}/result");
        let (status, _) = call(
            &app,
            "PUT",
            &uri,
            t,
            Some(json!({"score_a": 2, "score_b": 1})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    panic!("tournament never finished");
}

#[tokio::test]
async fn activity_log_is_public_ordered_and_marks_corrections() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Audit", "format": "single_elimination"})),
    )
    .await;
    let teams: Vec<_> = (1..=4).map(|n| json!({"name": format!("t{n}")})).collect();
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": teams})),
    )
    .await;
    call(&app, "POST", "/api/v1/admin/tournaments/1/start", t, None).await;
    let uri = |m: usize, tail: &str| format!("/api/v1/admin/tournaments/1/matches/{m}/{tail}");
    call(
        &app,
        "PUT",
        &uri(0, "result"),
        t,
        Some(json!({"score_a": 3, "score_b": 1})),
    )
    .await;
    call(
        &app,
        "PUT",
        &uri(0, "result"),
        t,
        Some(json!({"score_a": 1, "score_b": 3})),
    )
    .await;
    call(
        &app,
        "PUT",
        &uri(1, "forfeit"),
        t,
        Some(json!({"winner": "A"})),
    )
    .await;
    call(&app, "DELETE", &uri(1, "result"), t, None).await;
    // A refused edit must not leave a log line behind.
    call(
        &app,
        "PUT",
        &uri(2, "result"),
        t,
        Some(json!({"score_a": 1, "score_b": 0})),
    )
    .await;

    let (status, log) = call(
        &app,
        "GET",
        "/api/v1/tournaments/audit/activity",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<_> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["cleared", "forfeit", "corrected", "result", "started"]
    );
    let corrected = &log[2];
    assert_eq!(corrected["data"]["score_a"], 1);
    assert_eq!(corrected["data"]["winner"], "B");
    assert!(log[0]["at"].as_i64().unwrap() >= log[4]["at"].as_i64().unwrap());
    assert_eq!(log[3]["match_id"], 0);

    let (status, _) = call(&app, "GET", "/api/v1/tournaments/nope/activity", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    call(&app, "POST", "/api/v1/admin/tournaments/1/reset", t, None).await;
    let (_, after) = call(
        &app,
        "GET",
        "/api/v1/tournaments/audit/activity",
        None,
        None,
    )
    .await;
    assert_eq!(after[0]["kind"], "reset");
}

#[tokio::test]
async fn index_html_ships_the_saved_look_so_the_first_paint_matches() {
    let app = app().await;
    let token = login(&app).await;
    let look = json!({
        "site_name": "A</title><script>x</script>", "accent_light": "#aa2200", "accent_dark": "#ff8866",
        "radius": 14, "font": "serif", "scheme": "dark", "density": "compact", "tagline": "Hello"
    });
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/admin/settings",
        Some(&token),
        Some(look),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, kind, html) = raw(&app, "GET", "/t/anything", None, "").await;
    if status == StatusCode::NOT_FOUND {
        return; // the web app is not built in this checkout
    }
    assert_eq!(status, StatusCode::OK);
    assert!(kind.starts_with("text/html"));
    assert!(html.contains("data-scheme=\"dark\""), "{html}");
    assert!(html.contains("--color-accent:#aa2200"));
    assert!(html.contains("font-size:14.5px"));
    assert!(html.contains("id=\"kymo-appearance\""));
    assert!(
        !html.contains("</title><script>x"),
        "site name must be escaped"
    );
    assert!(!html.contains("<script>x</script>"));
}

#[tokio::test]
async fn demo_data_covers_every_format_and_state_and_can_be_loaded_twice() {
    let app = app().await;
    let token = login(&app).await;
    let (status, _) = call(&app, "POST", "/api/v1/admin/demo", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, first) = call(&app, "POST", "/api/v1/admin/demo", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["added"], 7);
    let (_, again) = call(&app, "POST", "/api/v1/admin/demo", Some(&token), None).await;
    assert_eq!(again["added"], 0);

    let (_, list) = call(&app, "GET", "/api/v1/tournaments", None, None).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 7);
    let formats: std::collections::HashSet<_> =
        list.iter().map(|t| t["format"].as_str().unwrap()).collect();
    assert_eq!(formats.len(), 4);
    let statuses: std::collections::HashSet<_> =
        list.iter().map(|t| t["status"].as_str().unwrap()).collect();
    assert_eq!(statuses, ["draft", "live", "done"].into_iter().collect());

    let (_, winter) = call(&app, "GET", "/api/v1/tournaments/winter-finals", None, None).await;
    assert!(winter["champion"].is_number());
    let (_, owls) = call(
        &app,
        "GET",
        "/api/v1/tournaments/night-owls-invitational/activity",
        None,
        None,
    )
    .await;
    let kinds: Vec<_> = owls
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"corrected") && kinds.contains(&"forfeit"),
        "{kinds:?}"
    );
    let (_, worlds) = call(&app, "GET", "/api/v1/tournaments/worlds-2026", None, None).await;
    assert!(worlds["progress"]["played"].as_u64().unwrap() >= 18);
}

#[tokio::test]
async fn schedule_is_validated_logged_kept_after_play_and_exported_as_a_calendar() {
    let app = app().await;
    let token = login(&app).await;
    let t = Some(token.as_str());
    call(
        &app,
        "POST",
        "/api/v1/tournaments",
        t,
        Some(json!({"name": "Plan, Cup", "format": "single_elimination"})),
    )
    .await;
    let teams: Vec<_> = ["a;b", "c", "d", "e"]
        .iter()
        .map(|n| json!({"name": n}))
        .collect();
    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/teams",
        t,
        Some(json!({"teams": teams})),
    )
    .await;
    call(&app, "POST", "/api/v1/admin/tournaments/1/start", t, None).await;
    let uri = "/api/v1/admin/tournaments/1/matches/0/schedule";

    let (status, _) = call(&app, "PUT", uri, None, Some(json!({"at": 1_800_000_000}))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for bad in [
        json!({"at": 5}),
        json!({"at": 99_999_999_999_i64}),
        json!({"venue": "x".repeat(41)}),
        json!({"venue": "a\u{202E}b"}),
    ] {
        let (status, _) = call(&app, "PUT", uri, t, Some(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/matches/99/schedule",
        t,
        Some(json!({"at": 1_800_000_000})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, out) = call(
        &app,
        "PUT",
        uri,
        t,
        Some(json!({"at": 1_800_000_000, "venue": "  Room, A  "})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["bracket"]["matches"][0]["venue"], "Room, A");

    call(
        &app,
        "PUT",
        "/api/v1/admin/tournaments/1/matches/0/result",
        t,
        Some(json!({"score_a": 1, "score_b": 0})),
    )
    .await;
    let (_, after) = call(&app, "GET", "/api/v1/tournaments/plan-cup", None, None).await;
    assert_eq!(
        after["bracket"]["matches"][0]["scheduled_at"],
        1_800_000_000
    );

    let (_, log) = call(
        &app,
        "GET",
        "/api/v1/tournaments/plan-cup/activity",
        None,
        None,
    )
    .await;
    let kinds: Vec<_> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["result", "scheduled", "started"],
        "scheduling a played match is not a correction"
    );

    let (status, kind, ics) = raw(
        &app,
        "GET",
        "/api/v1/tournaments/plan-cup/calendar.ics",
        None,
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(kind.starts_with("text/calendar"));
    assert!(ics.starts_with("BEGIN:VCALENDAR\r\n") && ics.ends_with("END:VCALENDAR\r\n"));
    assert!(ics.contains("DTSTART:20270115T080000Z"), "{ics}");
    assert!(ics.contains("SUMMARY:a\\;b vs e"), "{ics}");
    assert!(ics.contains("LOCATION:Room\\, A"));
    assert_eq!(ics.matches("BEGIN:VEVENT").count(), 1);

    let (status, cleared) = call(
        &app,
        "PUT",
        uri,
        t,
        Some(json!({"at": null, "venue": null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cleared["bracket"]["matches"][0]["scheduled_at"].is_null());
    let (_, _, empty) = raw(
        &app,
        "GET",
        "/api/v1/tournaments/plan-cup/calendar.ics",
        None,
        "",
    )
    .await;
    assert_eq!(empty.matches("BEGIN:VEVENT").count(), 0);
}

#[tokio::test]
async fn the_hundred_team_demo_tournament_has_unique_names_byes_and_progress() {
    let app = app().await;
    let token = login(&app).await;
    call(&app, "POST", "/api/v1/admin/demo", Some(&token), None).await;
    let (_, open) = call(
        &app,
        "GET",
        "/api/v1/tournaments/ctf-world-open",
        None,
        None,
    )
    .await;
    let names: std::collections::HashSet<_> = open["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names.len(), 100);
    assert_eq!(open["status"], "live");
    // 100 teams need 99 matches plus the third place match.
    assert_eq!(open["progress"]["total"], 100);
    assert!(open["progress"]["played"].as_u64().unwrap() >= 60);
    assert_eq!(open["bracket"]["matches"].as_array().unwrap().len(), 128);
}

#[tokio::test]
async fn only_embed_pages_may_be_framed() {
    let app = app().await;
    let header = |res: &axum::http::Response<Body>, name: &str| {
        res.headers()
            .get(name)
            .map(|v| v.to_str().unwrap().to_owned())
    };
    for path in ["/", "/t/some-slug", "/api/v1/tournaments"] {
        let res = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            header(&res, "x-frame-options").as_deref(),
            Some("DENY"),
            "{path}"
        );
        assert!(
            header(&res, "content-security-policy")
                .unwrap()
                .contains("frame-ancestors 'none'"),
            "{path}"
        );
        assert_eq!(
            header(&res, "x-content-type-options").as_deref(),
            Some("nosniff")
        );
    }
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/embed/some-slug")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(header(&res, "x-frame-options"), None);
    assert!(
        header(&res, "content-security-policy")
            .unwrap()
            .contains("frame-ancestors *")
    );
    assert_eq!(
        header(&res, "x-content-type-options").as_deref(),
        Some("nosniff")
    );
}
