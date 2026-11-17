use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

struct World {
    app: Router,
    clock: Arc<AtomicI64>,
}

impl World {
    async fn new(trust_proxy: bool) -> World {
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let clock = Arc::new(AtomicI64::new(T0));
        let shared = Arc::clone(&clock);
        let state = AppState::new(pool, PASSWORD)
            .unwrap()
            .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)))
            .with_trusted_proxy(trust_proxy);
        World {
            app: router(state),
            clock,
        }
    }

    fn set_time(&self, t: i64) {
        self.clock.store(t, Ordering::SeqCst);
    }

    /// One request. `ip` is the peer address; `extra` adds headers.
    async fn send(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        ip: Option<&str>,
        extra: &[(&str, &str)],
        body: Option<Value>,
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        if let Some(ip) = ip {
            let addr: SocketAddr = format!("{ip}:4000").parse().unwrap();
            req = req.extension(ConnectInfo(addr));
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let bytes = match &body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                v.to_string().into_bytes()
            }
            None => Vec::new(),
        };
        let res = self
            .app
            .clone()
            .oneshot(req.body(Body::from(bytes)).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let raw = res.into_body().collect().await.unwrap().to_bytes();
        Reply {
            status,
            headers,
            body: serde_json::from_slice(&raw).unwrap_or(Value::Null),
        }
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        ip: Option<&str>,
        body: Option<Value>,
    ) -> Reply {
        self.send(method, &format!("/api/v1{uri}"), token, ip, &[], body)
            .await
    }

    async fn admin(&self) -> String {
        let r = self
            .call(
                "POST",
                "/auth/login",
                None,
                Some("10.9.9.9"),
                Some(json!({ "password": PASSWORD })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        r.body["token"].as_str().unwrap().to_owned()
    }

    /// Registers a player and returns (token, id).
    async fn player(&self, name: &str) -> (String, i64) {
        let r = self
            .call(
                "POST",
                "/auth/register",
                None,
                Some("10.8.8.8"),
                Some(json!({ "username": name, "password": PLAYER_PASSWORD })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        (
            r.body["token"].as_str().unwrap().to_owned(),
            r.body["user"]["id"].as_i64().unwrap(),
        )
    }

    /// A player promoted to the given role by the admin.
    async fn staff(&self, admin: &str, name: &str, role: &str) -> (String, i64) {
        let (token, id) = self.player(name).await;
        let r = self
            .call(
                "PUT",
                &format!("/admin/users/{id}"),
                Some(admin),
                None,
                Some(json!({ "role": role })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        (token, id)
    }
}

fn rule(endpoint: &str, max: u32, strikes: Option<u32>, ban_secs: i64) -> Value {
    json!({
        "endpoint": endpoint,
        "max_requests": max,
        "window_secs": 60,
        "scope": "ip",
        "strikes_to_ban": strikes,
        "ban_secs": ban_secs,
    })
}

#[tokio::test]
async fn requests_are_logged_without_secrets_and_monitor_calls_are_not() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    let (alice, alice_id) = w.player("alice").await;
    let r = w
        .call("GET", "/me", Some(&alice), Some("10.0.0.5"), None)
        .await;
    assert_eq!(r.status, StatusCode::OK);
    w.call(
        "GET",
        "/settings?secret=hunter2",
        None,
        Some("10.0.0.6"),
        None,
    )
    .await;

    let r = w
        .call(
            "GET",
            "/admin/monitor/requests?ip=10.0.0.5",
            Some(&admin),
            None,
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let rows = r.body["requests"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["route"], "/api/v1/me");
    assert_eq!(rows[0]["method"], "GET");
    assert_eq!(rows[0]["status"], 200);
    assert_eq!(rows[0]["user_id"], alice_id);
    assert_eq!(rows[0]["username"], "alice");
    assert_eq!(rows[0]["at"], T0);

    let all = w
        .call(
            "GET",
            "/admin/monitor/requests?per_page=100",
            Some(&admin),
            None,
            None,
        )
        .await;
    let text = all.body.to_string();
    // A short query string and the user agent are kept, in their own fields, for attack
    // detection. Nothing else a caller sends is: no token, no body.
    let settings = all.body["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["route"] == "/api/v1/settings")
        .unwrap();
    assert_eq!(settings["query"], "secret=hunter2");
    assert!(!settings["path"].as_str().unwrap().contains('?'));
    assert!(!text.contains(&alice), "tokens are not stored");
    assert!(
        !text.contains("/admin/monitor"),
        "monitor reads are not logged"
    );
    assert!(text.contains("\"route\":\"/api/v1/settings\""));

    let by_user = w
        .call(
            "GET",
            &format!("/admin/monitor/requests?user_id={alice_id}"),
            Some(&admin),
            None,
            None,
        )
        .await;
    assert!(
        by_user.body["requests"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["user_id"] == alice_id)
    );
}

#[tokio::test]
async fn only_staff_roles_reach_the_monitor() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    let (player, _) = w.player("pat").await;
    let (monitor, _) = w.staff(&admin, "mona", "monitor").await;
    let (referee, _) = w.staff(&admin, "rex", "referee").await;

    for path in [
        "/admin/monitor/requests",
        "/admin/monitor/rules",
        "/admin/monitor/bans",
        "/admin/monitor/audit",
        "/admin/monitor/summary",
    ] {
        assert_eq!(
            w.call("GET", path, None, None, None).await.status,
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
        assert_eq!(
            w.call("GET", path, Some(&player), None, None).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
        assert_eq!(
            w.call("GET", path, Some(&referee), None, None).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
        assert_eq!(
            w.call("GET", path, Some(&monitor), None, None).await.status,
            StatusCode::OK,
            "{path}"
        );
    }
    let made = w
        .call(
            "POST",
            "/admin/monitor/rules",
            Some(&monitor),
            None,
            Some(rule("*", 5, None, 0)),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "monitors may moderate");
    let denied = w
        .call(
            "POST",
            "/admin/monitor/rules",
            Some(&player),
            None,
            Some(rule("*", 5, None, 0)),
        )
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_rate_rule_throttles_one_address_and_can_be_removed() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    let made = w
        .call(
            "POST",
            "/admin/monitor/rules",
            Some(&admin),
            None,
            Some(rule("GET /api/v1/settings", 2, None, 0)),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let id = made.body["id"].as_i64().unwrap();

    let hit = |ip: &'static str| w.call("GET", "/settings", None, Some(ip), None);
    assert_eq!(hit("10.0.0.1").await.status, StatusCode::OK);
    assert_eq!(hit("10.0.0.1").await.status, StatusCode::OK);
    let third = hit("10.0.0.1").await;
    assert_eq!(third.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(third.headers["retry-after"], "60");
    assert_eq!(
        hit("10.0.0.2").await.status,
        StatusCode::OK,
        "another address has its own budget"
    );

    w.set_time(T0 + 60);
    assert_eq!(
        hit("10.0.0.1").await.status,
        StatusCode::OK,
        "the window slid"
    );

    let gone = w
        .call(
            "DELETE",
            &format!("/admin/monitor/rules/{id}"),
            Some(&admin),
            None,
            None,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    for _ in 0..4 {
        assert_eq!(hit("10.0.0.1").await.status, StatusCode::OK);
    }
}

#[tokio::test]
async fn repeated_denials_ban_the_address_until_the_ban_expires() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    w.call(
        "POST",
        "/admin/monitor/rules",
        Some(&admin),
        None,
        Some(rule("GET /api/v1/settings", 1, Some(2), 100)),
    )
    .await;
    let hit = || w.call("GET", "/settings", None, Some("10.0.0.7"), None);
    assert_eq!(hit().await.status, StatusCode::OK);
    assert_eq!(hit().await.status, StatusCode::TOO_MANY_REQUESTS);
    let banned = hit().await;
    assert_eq!(banned.status, StatusCode::FORBIDDEN, "{}", banned.body);
    assert_eq!(banned.body["error"], "this address is banned");
    let other = w
        .call("GET", "/settings", None, Some("10.0.0.7"), None)
        .await;
    assert_eq!(
        other.status,
        StatusCode::FORBIDDEN,
        "the ban covers every endpoint"
    );

    let bans = w
        .call("GET", "/admin/monitor/bans", Some(&admin), None, None)
        .await;
    let ban = &bans.body["bans"][0];
    assert_eq!(ban["subject"], "10.0.0.7");
    assert_eq!(ban["auto"], true);
    assert_eq!(ban["active"], true);
    assert!(
        ban["reason"]
            .as_str()
            .unwrap()
            .contains("GET /api/v1/settings")
    );

    let audit = w
        .call(
            "GET",
            "/admin/monitor/audit?action=auto_ban",
            Some(&admin),
            None,
            None,
        )
        .await;
    assert_eq!(audit.body["total"], 1);
    assert_eq!(audit.body["entries"][0]["actor_role"], "system");

    w.set_time(T0 + 100);
    assert_eq!(
        w.call("GET", "/settings", None, Some("10.0.0.7"), None)
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn an_unknown_address_is_throttled_but_never_banned() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    w.call(
        "POST",
        "/admin/monitor/rules",
        Some(&admin),
        None,
        Some(rule("GET /api/v1/settings", 1, Some(1), 0)),
    )
    .await;
    assert_eq!(
        w.call("GET", "/settings", None, None, None).await.status,
        StatusCode::OK
    );
    for _ in 0..3 {
        let r = w.call("GET", "/settings", None, None, None).await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    }
    let bans = w
        .call("GET", "/admin/monitor/bans", Some(&admin), None, None)
        .await;
    assert_eq!(bans.body["bans"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn manual_ip_bans_block_everyone_but_staff_and_are_audited() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    let made = w
        .call(
            "POST",
            "/admin/monitor/bans",
            Some(&admin),
            Some("10.1.1.1"),
            Some(json!({ "kind": "ip", "subject": "10.0.0.9", "reason": "scanning", "duration_secs": 3600 })),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let id = made.body["id"].as_i64().unwrap();

    assert_eq!(
        w.call("GET", "/settings", None, Some("10.0.0.9"), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let staff = w
        .call(
            "GET",
            "/admin/monitor/bans",
            Some(&admin),
            Some("10.0.0.9"),
            None,
        )
        .await;
    assert_eq!(
        staff.status,
        StatusCode::OK,
        "staff are never locked out by an address ban"
    );

    let own = w
        .call(
            "POST",
            "/admin/monitor/bans",
            Some(&admin),
            Some("10.1.1.1"),
            Some(json!({ "kind": "ip", "subject": "10.1.1.1", "reason": "oops" })),
        )
        .await;
    assert_eq!(own.status, StatusCode::CONFLICT, "{}", own.body);

    let lifted = w
        .call(
            "DELETE",
            &format!("/admin/monitor/bans/{id}"),
            Some(&admin),
            None,
            None,
        )
        .await;
    assert_eq!(lifted.status, StatusCode::OK, "{}", lifted.body);
    assert!(lifted.body["lifted_at"].is_number());
    assert_eq!(
        w.call("GET", "/settings", None, Some("10.0.0.9"), None)
            .await
            .status,
        StatusCode::OK
    );
    let again = w
        .call(
            "DELETE",
            &format!("/admin/monitor/bans/{id}"),
            Some(&admin),
            None,
            None,
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);

    let audit = w
        .call("GET", "/admin/monitor/audit", Some(&admin), None, None)
        .await;
    let actions: Vec<&str> = audit.body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(
        actions.contains(&"ban_create") && actions.contains(&"ban_lift"),
        "{actions:?}"
    );
    let first = audit.body["entries"].as_array().unwrap().last().unwrap();
    assert_eq!(first["action"], "ban_create");
    assert_eq!(first["target_id"], "10.0.0.9");
    assert_eq!(first["detail"], "scanning");
    assert_eq!(first["actor_role"], "admin");
}

#[tokio::test]
async fn banning_a_user_ends_their_sessions_and_staff_cannot_be_banned() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    let (token, id) = w.player("troll").await;
    let (_, mod_id) = w.staff(&admin, "mona", "monitor").await;

    let made = w
        .call(
            "POST",
            "/admin/monitor/bans",
            Some(&admin),
            None,
            Some(json!({ "kind": "user", "subject": id.to_string(), "reason": "abuse" })),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    assert_eq!(
        w.call("GET", "/me", Some(&token), None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let login = w
        .call(
            "POST",
            "/auth/player-login",
            None,
            Some("10.3.3.3"),
            Some(json!({ "username": "troll", "password": PLAYER_PASSWORD })),
        )
        .await;
    assert_eq!(login.status, StatusCode::FORBIDDEN);

    let ban_id = made.body["id"].as_i64().unwrap();
    w.call(
        "DELETE",
        &format!("/admin/monitor/bans/{ban_id}"),
        Some(&admin),
        None,
        None,
    )
    .await;
    let login = w
        .call(
            "POST",
            "/auth/player-login",
            None,
            Some("10.3.3.3"),
            Some(json!({ "username": "troll", "password": PLAYER_PASSWORD })),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.body);

    for target in [mod_id.to_string(), "9999".to_string()] {
        let r = w
            .call(
                "POST",
                "/admin/monitor/bans",
                Some(&admin),
                None,
                Some(json!({ "kind": "user", "subject": target, "reason": "x" })),
            )
            .await;
        assert!(
            matches!(r.status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
            "{target}: {}",
            r.status
        );
    }
}

#[tokio::test]
async fn rules_and_bans_validate_their_input() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    for bad in [
        json!({ "endpoint": "no-slash", "max_requests": 5, "window_secs": 60 }),
        json!({ "endpoint": "FETCH /x", "max_requests": 5, "window_secs": 60 }),
        json!({ "endpoint": "/x", "max_requests": 0, "window_secs": 60 }),
        json!({ "endpoint": "/x", "max_requests": 5, "window_secs": 0 }),
        json!({ "endpoint": "/x", "max_requests": 5, "window_secs": 60, "scope": "team" }),
        json!({ "endpoint": "/x", "max_requests": 5, "window_secs": 60, "ban_secs": -4 }),
    ] {
        let r = w
            .call(
                "POST",
                "/admin/monitor/rules",
                Some(&admin),
                None,
                Some(bad.clone()),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    for bad in [
        json!({ "kind": "ip", "subject": "not an ip", "reason": "x" }),
        json!({ "kind": "ip", "subject": "10.0.0.1", "reason": "  " }),
        json!({ "kind": "ip", "subject": "10.0.0.1", "reason": "x", "duration_secs": 0 }),
        json!({ "kind": "ip", "subject": "10.0.0.1", "reason": "x", "duration_secs": 99999999999i64 }),
        json!({ "kind": "domain", "subject": "a.b", "reason": "x" }),
    ] {
        let r = w
            .call(
                "POST",
                "/admin/monitor/bans",
                Some(&admin),
                None,
                Some(bad.clone()),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    let seeded = w
        .call("GET", "/admin/monitor/rules", Some(&admin), None, None)
        .await;
    let endpoints: Vec<&str> = seeded.body["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["endpoint"].as_str().unwrap())
        .collect();
    assert!(
        endpoints.contains(&"POST /api/v1/auth/player-login"),
        "{endpoints:?}"
    );
}

#[tokio::test]
async fn moderation_actions_land_in_the_audit_trail() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    let (_, id) = w.player("quinn").await;
    w.call(
        "PUT",
        &format!("/admin/users/{id}"),
        Some(&admin),
        None,
        Some(json!({ "banned": true })),
    )
    .await;
    let ev = w
        .call(
            "POST",
            "/admin/events",
            Some(&admin),
            None,
            Some(json!({ "name": "Audit CTF", "kind": "jeopardy", "mode": "solo" })),
        )
        .await;
    let event_id = ev.body["id"].as_i64().unwrap();
    w.call(
        "POST",
        &format!("/admin/events/{event_id}/state"),
        Some(&admin),
        None,
        Some(json!({ "state": "registration_open" })),
    )
    .await;

    let audit = w
        .call(
            "GET",
            "/admin/monitor/audit?per_page=100",
            Some(&admin),
            None,
            None,
        )
        .await;
    let entries = audit.body["entries"].as_array().unwrap();
    let find = |action: &str| entries.iter().find(|e| e["action"] == action).cloned();
    let user = find("user_update").expect("user_update");
    assert_eq!(user["target_id"], id.to_string());
    assert!(user["detail"].as_str().unwrap().contains("banned=true"));
    let state = find("event_state").expect("event_state");
    assert_eq!(state["detail"], "registration_open");

    let none = w
        .call(
            "GET",
            "/admin/monitor/audit?action=nothing",
            Some(&admin),
            None,
            None,
        )
        .await;
    assert_eq!(none.body["total"], 0);
}

#[tokio::test]
async fn summary_counts_traffic_and_monitors_read_submission_history() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    let (mon, _) = w.staff(&admin, "mona", "monitor").await;
    let (player, _) = w.player("pat").await;
    for _ in 0..3 {
        w.call("GET", "/settings", None, Some("10.0.0.3"), None)
            .await;
    }
    w.call("GET", "/me", None, Some("10.0.0.3"), None).await;

    let s = w
        .call(
            "GET",
            "/admin/monitor/summary?minutes=5",
            Some(&mon),
            None,
            None,
        )
        .await;
    assert_eq!(s.status, StatusCode::OK, "{}", s.body);
    assert!(s.body["total"].as_i64().unwrap() >= 4);
    assert_eq!(s.body["top_ips"][0]["key"], "10.0.0.3");
    assert_eq!(s.body["top_routes"][0]["key"], "/api/v1/settings");
    assert!(s.body["forbidden"].is_number() && s.body["throttled"].is_number());

    let ev = w
        .call(
            "POST",
            "/admin/events",
            Some(&admin),
            None,
            Some(json!({ "name": "Flags", "kind": "jeopardy", "mode": "solo" })),
        )
        .await;
    let id = ev.body["id"].as_i64().unwrap();
    let uri = format!("/admin/monitor/events/{id}/submissions");
    assert_eq!(
        w.call("GET", &uri, Some(&mon), None, None).await.status,
        StatusCode::OK
    );
    assert_eq!(
        w.call("GET", &uri, Some(&player), None, None).await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn the_forwarded_address_is_used_only_behind_a_trusted_proxy() {
    for (trusted, expected) in [(true, "9.9.9.9"), (false, "10.0.0.4")] {
        let w = World::new(trusted).await;
        let admin = w.admin().await;
        let r = w
            .send(
                "GET",
                "/api/v1/settings",
                None,
                Some("10.0.0.4"),
                &[("x-forwarded-for", "1.1.1.1, 9.9.9.9")],
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        let log = w
            .call(
                "GET",
                "/admin/monitor/requests?route=/api/v1/settings",
                Some(&admin),
                None,
                None,
            )
            .await;
        assert_eq!(log.body["requests"][0]["ip"], expected, "trusted={trusted}");
    }
}

#[tokio::test]
async fn a_burst_of_requests_is_logged_completely_by_the_background_writer() {
    let w = World::new(false).await;
    let admin = w.admin().await;
    for _ in 0..250 {
        let r = w
            .call("GET", "/settings", None, Some("10.0.0.60"), None)
            .await;
        assert_eq!(r.status, StatusCode::OK);
    }
    // Readers wait for queued rows first, so what they see is complete and current.
    let log = w
        .call(
            "GET",
            "/admin/monitor/requests?ip=10.0.0.60&per_page=100",
            Some(&admin),
            None,
            None,
        )
        .await;
    assert_eq!(log.body["total"], 250, "{}", log.body["total"]);
    assert_eq!(log.body["requests"].as_array().unwrap().len(), 100);
    let summary = w
        .call(
            "GET",
            "/admin/monitor/summary?minutes=5",
            Some(&admin),
            None,
            None,
        )
        .await;
    let busiest = summary.body["top_routes"].as_array().unwrap();
    assert!(
        busiest
            .iter()
            .any(|r| r["key"] == "/api/v1/settings" && r["count"] == 250),
        "{busiest:?}"
    );
    // The health report is current too, and cleanup counts the rows that were queued.
    let health = w
        .call("GET", "/admin/ops/health", Some(&admin), None, None)
        .await;
    assert!(health.body["traffic"]["requests"].as_i64().unwrap() >= 250);
}
