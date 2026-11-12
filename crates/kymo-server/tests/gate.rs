//! The sign in rule: what a visitor without a session may reach, and what they may not.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const T0: i64 = 1_767_225_600;

struct Site {
    app: Router,
    pool: SqlitePool,
    clock: Arc<AtomicI64>,
}

impl Site {
    async fn new() -> Self {
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let clock = Arc::new(AtomicI64::new(T0));
        let shared = Arc::clone(&clock);
        let state = AppState::new(pool.clone(), PASSWORD)
            .unwrap()
            .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
        Self {
            app: router(state),
            pool,
            clock,
        }
    }

    async fn call(
        &self,
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
        let res = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Only the status: for a live stream whose body never ends.
    async fn status(&self, uri: &str) -> StatusCode {
        let req = Request::builder()
            .uri(format!("/api/v1{uri}"))
            .body(Body::empty())
            .unwrap();
        self.app.clone().oneshot(req).await.unwrap().status()
    }

    async fn admin(&self) -> String {
        let (s, b) = self
            .call(
                "POST",
                "/auth/login",
                None,
                Some(json!({ "password": PASSWORD })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["token"].as_str().unwrap().to_owned()
    }

    async fn player(&self, name: &str) -> String {
        let (s, b) = self
            .call(
                "POST",
                "/auth/register",
                None,
                Some(json!({ "username": name, "password": "a long enough password" })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["token"].as_str().unwrap().to_owned()
    }

    /// A Jeopardy event in the given stored state, with its scoreboard public or not.
    async fn event(&self, admin: &str, name: &str, state: &str, public: bool) -> (i64, String) {
        let (s, b) = self
            .call(
                "POST",
                "/admin/events",
                Some(admin),
                Some(json!({ "name": name, "kind": "jeopardy", "mode": "solo" })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        let id = b["id"].as_i64().unwrap();
        sqlx::query("UPDATE event SET state = ?, scoreboard_public = ? WHERE id = ?")
            .bind(state)
            .bind(public)
            .bind(id)
            .execute(&self.pool)
            .await
            .unwrap();
        (id, b["slug"].as_str().unwrap().to_owned())
    }
}

#[tokio::test]
async fn a_visitor_reaches_only_the_sign_in_pages() {
    let site = Site::new().await;
    for open in ["/settings", "/home", "/registration", "/public/scoreboards"] {
        let (s, b) = site.call("GET", open, None, None).await;
        assert_eq!(s, StatusCode::OK, "{open}: {b}");
    }
    for closed in [
        "/events",
        "/events/anything",
        "/events/anything/challenges",
        "/tournaments",
        "/ladder",
        "/ladder/timeline",
        "/me",
        "/vpn",
        "/me/instance",
        "/admin/events",
        "/not/a/route",
    ] {
        let (s, _) = site.call("GET", closed, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{closed}");
    }
    // Writing to the open pages is not open.
    let (s, _) = site
        .call("PUT", "/admin/home", None, Some(json!({ "headline": "x" })))
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_session_opens_the_rest_and_a_stale_one_does_not() {
    let site = Site::new().await;
    let admin = site.admin().await;
    let pat = site.player("pat").await;
    for token in [&admin, &pat] {
        for path in ["/events", "/tournaments", "/ladder"] {
            let (s, b) = site.call("GET", path, Some(token), None).await;
            assert_eq!(s, StatusCode::OK, "{path}: {b}");
        }
    }
    let (s, _) = site
        .call("GET", "/events", Some("not-a-real-token"), None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // A stale token on the sign in page does not stop anyone from signing in.
    let (s, b) = site
        .call(
            "POST",
            "/auth/login",
            Some("not-a-real-token"),
            Some(json!({ "password": PASSWORD })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
}

#[tokio::test]
async fn a_public_scoreboard_is_open_and_shows_nothing_else() {
    let site = Site::new().await;
    let admin = site.admin().await;
    let (_, open) = site.event(&admin, "Open Cup", "live", true).await;
    let (_, closed) = site.event(&admin, "Closed Cup", "live", false).await;
    let (_, draft) = site.event(&admin, "Draft Cup", "draft", true).await;

    let (s, b) = site
        .call("GET", &format!("/events/{open}/scoreboard"), None, None)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = site
        .call(
            "GET",
            &format!("/events/{open}/scoreboard/timeline"),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = site
        .call("GET", &format!("/public/events/{open}"), None, None)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["name"], "Open Cup");
    assert!(b.get("description").is_none(), "{b}");

    // Everything else about it is for people who sign in.
    for path in [
        format!("/events/{open}"),
        format!("/events/{open}/challenges"),
        format!("/events/{open}/rules"),
        format!("/events/{open}/progress"),
    ] {
        let (s, _) = site.call("GET", &path, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{path}");
    }
    // A scoreboard that is not public, or an event that is still a draft, is not open.
    for slug in [&closed, &draft] {
        for path in [
            format!("/events/{slug}/scoreboard"),
            format!("/public/events/{slug}"),
        ] {
            let (s, _) = site.call("GET", &path, None, None).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{path}");
        }
    }
    // A stale session on a public view reads as signed out.
    let (s, _) = site
        .call(
            "GET",
            &format!("/events/{open}/scoreboard"),
            Some("stale"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // The home page lists the public ones, and only those.
    let (_, list) = site.call("GET", "/public/scoreboards", None, None).await;
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Open Cup"], "{list}");

    // The live update ping carries no data, and a browser cannot send the session header.
    assert_eq!(
        site.status(&format!("/events/{closed}/stream")).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_public_tournament_is_readable_for_embeds_and_a_private_one_is_not() {
    let site = Site::new().await;
    let admin = site.admin().await;
    let mut slugs = Vec::new();
    for name in ["Public Cup", "Private Cup"] {
        let (s, t) = site
            .call(
                "POST",
                "/tournaments",
                Some(&admin),
                Some(json!({ "name": name, "format": "single_elimination" })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{t}");
        slugs.push((
            t["slug"].as_str().unwrap().to_owned(),
            t["event_id"].as_i64().unwrap(),
        ));
    }
    // A draft is never public, even with the switch on; this one is running.
    sqlx::query("UPDATE event SET scoreboard_public = 1, state = 'live' WHERE id = ?")
        .bind(slugs[0].1)
        .execute(&site.pool)
        .await
        .unwrap();
    let (public, private) = (&slugs[0].0, &slugs[1].0);
    for path in [
        format!("/tournaments/{public}"),
        format!("/tournaments/{public}/activity"),
        format!("/tournaments/{public}/calendar.ics"),
    ] {
        let (s, _) = site.call("GET", &path, None, None).await;
        assert_eq!(s, StatusCode::OK, "{path}");
    }
    for path in [
        format!("/tournaments/{private}"),
        format!("/tournaments/{private}/activity"),
        format!("/tournaments/{public}/play/mine"),
        "/tournaments".to_owned(),
    ] {
        let (s, _) = site.call("GET", &path, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{path}");
    }
    // Reading is all a visitor gets.
    let (s, _) = site
        .call(
            "PUT",
            "/admin/tournaments/1/teams",
            None,
            Some(json!({ "teams": [] })),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(
        site.status(&format!("/tournaments/{private}/events")).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn answers_to_visitors_are_reused_for_a_few_seconds() {
    let site = Site::new().await;
    let admin = site.admin().await;
    let (id, slug) = site.event(&admin, "First Name", "live", true).await;
    let path = format!("/public/events/{slug}");
    let (_, first) = site.call("GET", &path, None, None).await;
    assert_eq!(first["name"], "First Name");
    sqlx::query("UPDATE event SET name = 'Second Name' WHERE id = ?")
        .bind(id)
        .execute(&site.pool)
        .await
        .unwrap();
    site.clock.store(T0 + 2, Ordering::SeqCst);
    let (_, cached) = site.call("GET", &path, None, None).await;
    assert_eq!(
        cached["name"], "First Name",
        "a crowd costs one computation, not one each"
    );
    // Someone signed in always gets the live answer.
    let pat = site.player("pat").await;
    let (_, live) = site.call("GET", &path, Some(&pat), None).await;
    assert_eq!(live["name"], "Second Name");
    site.clock.store(T0 + 6, Ordering::SeqCst);
    let (_, fresh) = site.call("GET", &path, None, None).await;
    assert_eq!(fresh["name"], "Second Name");
}

#[tokio::test]
async fn the_home_page_is_open_to_read_and_the_organizer_edits_it_safely() {
    let site = Site::new().await;
    let admin = site.admin().await;
    let pat = site.player("pat").await;
    let (s, home) = site.call("GET", "/home", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(home["mode"], "default");
    assert_eq!(home["signin_label"], "Sign in");

    // A player cannot change it.
    let (s, _) = site
        .call("PUT", "/admin/home", Some(&pat), Some(home.clone()))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // The organizer changes the texts.
    let mut edited = home.clone();
    edited["headline"] = json!("Welcome to Dakar");
    edited["banner"] = json!("Registration closes Friday");
    let (s, b) = site
        .call("PUT", "/admin/home", Some(&admin), Some(edited))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, now) = site.call("GET", "/home", None, None).await;
    assert_eq!(now["headline"], "Welcome to Dakar");
    assert_eq!(now["banner"], "Registration closes Friday");

    // Their own HTML is cleaned when saved; CSS that loads from outside is refused.
    let mut custom = home.clone();
    custom["mode"] = json!("custom");
    custom["custom_html"] =
        json!(r#"<h1 onclick="x()">Hi</h1><script>alert(1)</script><a href="/signin">Go</a>"#);
    custom["custom_css"] = json!("h1 { color: var(--color-accent); }");
    let (s, b) = site
        .call("PUT", "/admin/home", Some(&admin), Some(custom.clone()))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let html = b["custom_html"].as_str().unwrap();
    assert!(
        !html.contains("script") && !html.contains("onclick"),
        "{html}"
    );
    assert!(html.contains(r#"href="/signin""#), "{html}");
    custom["custom_css"] = json!("@import url(https://evil.example/x.css);");
    let (s, _) = site
        .call("PUT", "/admin/home", Some(&admin), Some(custom))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Back to the default.
    let (s, b) = site.call("DELETE", "/admin/home", Some(&admin), None).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, back) = site.call("GET", "/home", None, None).await;
    assert_eq!(back["mode"], "default");
    assert_eq!(back["headline"], home["headline"]);
}

#[tokio::test]
async fn each_event_has_its_own_rules() {
    let site = Site::new().await;
    let admin = site.admin().await;
    let pat = site.player("pat").await;
    let (a, a_slug) = site.event(&admin, "Cup A", "live", false).await;
    let (_, b_slug) = site.event(&admin, "Cup B", "live", false).await;

    let (s, none) = site
        .call("GET", &format!("/events/{a_slug}/rules"), Some(&pat), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(none["body"], "");

    // Only organizers write them.
    let rules = json!({ "title": "Code of conduct", "body": "# Fair play\n- no sharing flags" });
    let (s, _) = site
        .call(
            "PUT",
            &format!("/admin/events/{a}/rules"),
            Some(&pat),
            Some(rules.clone()),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, b) = site
        .call(
            "PUT",
            &format!("/admin/events/{a}/rules"),
            Some(&admin),
            Some(rules),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");

    let (_, got) = site
        .call("GET", &format!("/events/{a_slug}/rules"), Some(&pat), None)
        .await;
    assert_eq!(got["title"], "Code of conduct");
    assert!(got["body"].as_str().unwrap().contains("no sharing flags"));
    // Another event is untouched.
    let (_, other) = site
        .call("GET", &format!("/events/{b_slug}/rules"), Some(&pat), None)
        .await;
    assert_eq!(other["body"], "");

    // Too long is refused, and an empty text removes them.
    let (s, _) = site
        .call(
            "PUT",
            &format!("/admin/events/{a}/rules"),
            Some(&admin),
            Some(json!({ "title": "Rules", "body": "x".repeat(20_001) })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = site
        .call(
            "PUT",
            &format!("/admin/events/{a}/rules"),
            Some(&admin),
            Some(json!({ "title": "", "body": "  " })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, gone) = site
        .call("GET", &format!("/events/{a_slug}/rules"), Some(&pat), None)
        .await;
    assert_eq!(gone, json!({ "title": "Rules", "body": "" }));
}
