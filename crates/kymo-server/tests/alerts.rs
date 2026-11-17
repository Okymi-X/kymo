//! Attack alerts, automatic bans and the numbers behind the monitor's live graphs.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, monitor_alerts, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long enough password";
const T0: i64 = 1_767_225_600;

struct Site {
    app: Router,
    state: AppState,
    clock: Arc<AtomicI64>,
    admin: String,
}

impl Site {
    async fn new() -> Self {
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let clock = Arc::new(AtomicI64::new(T0));
        let shared = Arc::clone(&clock);
        let state = AppState::new(pool, PASSWORD)
            .unwrap()
            .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
        let mut site = Self {
            app: router(state.clone()),
            state,
            clock,
            admin: String::new(),
        };
        let (_, b) = site
            .send(
                "POST",
                "/auth/login",
                None,
                Some(json!({ "password": PASSWORD })),
                "10.9.9.9",
                "",
            )
            .await;
        site.admin = b["token"].as_str().unwrap().to_owned();
        site
    }

    async fn send(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
        ip: &str,
        ua: &str,
    ) -> (StatusCode, Value) {
        let addr: SocketAddr = format!("{ip}:4000").parse().unwrap();
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"))
            .extension(ConnectInfo(addr));
        if !ua.is_empty() {
            req = req.header("user-agent", ua);
        }
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

    async fn get(&self, uri: &str, token: &str) -> (StatusCode, Value) {
        self.send("GET", uri, Some(token), None, "10.1.1.1", "")
            .await
    }

    async fn admin_call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let admin = self.admin.clone();
        self.send(method, uri, Some(&admin), body, "10.9.9.9", "")
            .await
    }

    async fn player(&self, name: &str) -> (String, i64) {
        let (s, b) = self
            .send(
                "POST",
                "/auth/register",
                None,
                Some(json!({ "username": name, "password": PLAYER_PASSWORD })),
                "10.2.2.2",
                "",
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        (
            b["token"].as_str().unwrap().to_owned(),
            b["user"]["id"].as_i64().unwrap(),
        )
    }

    /// A player looking for something to attack: that many different addresses that do not exist.
    async fn scan(&self, token: &str, ip: &str, n: usize) {
        for i in 0..n {
            self.send(
                "GET",
                &format!("/nothing-here-{i}"),
                Some(token),
                None,
                ip,
                "Mozilla/5.0",
            )
            .await;
        }
    }

    async fn detect(&self) -> monitor_alerts::DetectionReport {
        monitor_alerts::run_once(&self.state).await.unwrap()
    }

    async fn alerts(&self, query: &str) -> Value {
        let (s, b) = self
            .admin_call("GET", &format!("/admin/monitor/alerts?{query}"), None)
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b
    }

    fn advance(&self, secs: i64) {
        self.clock.fetch_add(secs, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn a_scanning_player_raises_an_alert_and_an_ordinary_one_and_staff_do_not() {
    let site = Site::new().await;
    let (pat, pat_id) = site.player("pat").await;
    let (sam, _) = site.player("sam").await;
    site.scan(&pat, "10.3.3.3", 30).await;
    for _ in 0..30 {
        site.get("/events", &sam).await;
    }
    // The organizer pokes at addresses too: staff are never flagged.
    site.scan(&site.admin.clone(), "10.9.9.9", 40).await;

    let report = site.detect().await;
    assert_eq!((report.raised, report.banned), (1, 0), "{report:?}");
    let page = site.alerts("status=open").await;
    let alerts = page["alerts"].as_array().unwrap();
    assert_eq!(alerts.len(), 1, "{page}");
    let alert = &alerts[0];
    assert_eq!(alert["kind"], "scan");
    assert_eq!(alert["user_id"], pat_id);
    assert_eq!(alert["username"], "pat");
    assert_eq!(alert["count"], 30);
    assert_eq!(alert["severity"], "medium", "below the automatic ban level");
    assert_eq!(alert["status"], "open");
    assert!(alert["evidence"].as_array().unwrap().len() >= 3);
    assert_eq!(
        (page["open_players"].clone(), page["open_all"].clone()),
        (json!(1), json!(1))
    );

    // It still works: below the ban level the player carries on.
    let (s, _) = site.get("/events", &pat).await;
    assert_eq!(s, StatusCode::OK);
    // Running again with nothing new grows nothing and raises nothing twice.
    let again = site.detect().await;
    assert_eq!((again.raised, again.banned), (0, 0));
    assert_eq!(site.alerts("status=open").await["total"], 1);
}

#[tokio::test]
async fn reaching_the_peak_bans_the_player_for_a_while_and_then_lets_them_back() {
    let site = Site::new().await;
    let (pat, pat_id) = site.player("pat").await;
    site.scan(&pat, "10.3.3.3", 130).await;

    let report = site.detect().await;
    assert_eq!(report.banned, 1, "{report:?}");
    let alert = &site.alerts("status=handled").await["alerts"][0];
    assert_eq!(
        (alert["status"].clone(), alert["severity"].clone()),
        (json!("banned"), json!("high"))
    );
    // The session is gone and the account cannot sign in.
    let (s, _) = site.get("/events", &pat).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = site
        .send(
            "POST",
            "/auth/player-login",
            None,
            Some(json!({ "username": "pat", "password": PLAYER_PASSWORD })),
            "10.2.2.2",
            "",
        )
        .await;
    assert_ne!(s, StatusCode::OK, "banned for now");
    let (_, bans) = site.admin_call("GET", "/admin/monitor/bans", None).await;
    let ban = &bans["bans"][0];
    assert_eq!(
        (ban["auto"].clone(), ban["subject"].clone()),
        (json!(true), json!(pat_id.to_string()))
    );
    assert!(
        ban["expires_at"].is_number(),
        "a ban of a few minutes, not for ever"
    );

    // The ban runs out; nobody has to lift it.
    site.advance(16 * 60);
    let report = site.detect().await;
    assert_eq!(report.unbanned, 1, "{report:?}");
    let (s, b) = site
        .send(
            "POST",
            "/auth/player-login",
            None,
            Some(json!({ "username": "pat", "password": PLAYER_PASSWORD })),
            "10.2.2.2",
            "",
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
}

#[tokio::test]
async fn visitors_are_alerted_about_but_never_banned_by_address() {
    let site = Site::new().await;
    // A scanner tool and attack strings, from visitors who are not signed in.
    for _ in 0..50 {
        site.send(
            "GET",
            "/settings",
            None,
            None,
            "10.4.4.4",
            "sqlmap/1.7 (https://sqlmap.org)",
        )
        .await;
    }
    for _ in 0..3 {
        site.send(
            "GET",
            "/settings?x=../../etc/passwd",
            None,
            None,
            "10.5.5.5",
            "Mozilla/5.0",
        )
        .await;
    }
    let report = site.detect().await;
    assert_eq!(report.banned, 0, "an address may be a whole network");
    let page = site.alerts("status=open").await;
    let kinds: Vec<&str> = page["alerts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"scanner_tool") && kinds.contains(&"signature"),
        "{kinds:?}"
    );
    assert!(
        page["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["severity"] == "low" || a["severity"] == "high")
    );
    // The badge counts players, not visitors.
    assert_eq!(
        (
            page["open_players"].clone(),
            page["open_all"].as_i64().unwrap() >= 2
        ),
        (json!(0), true)
    );
    let only_players = site.alerts("status=open&who=players").await;
    assert_eq!(only_players["total"], 0);
    let (_, bans) = site.admin_call("GET", "/admin/monitor/bans", None).await;
    assert!(bans["bans"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_organizer_reads_the_evidence_dismisses_or_bans_and_nothing_is_raised_twice() {
    let site = Site::new().await;
    let (pat, pat_id) = site.player("pat").await;
    let (lee, lee_id) = site.player("lee").await;
    site.scan(&pat, "10.3.3.3", 30).await;
    site.scan(&lee, "10.3.3.4", 30).await;
    site.detect().await;
    let page = site.alerts("status=open").await;
    let find = |user: i64| {
        page["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["user_id"] == user)
            .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    let (pat_alert, lee_alert) = (find(pat_id), find(lee_id));

    // The details show the requests behind the alert.
    let (s, details) = site
        .admin_call("GET", &format!("/admin/monitor/alerts/{pat_alert}"), None)
        .await;
    assert_eq!(s, StatusCode::OK, "{details}");
    assert_eq!(details["request_total"], 30);
    assert!(
        details["requests"][0]["path"]
            .as_str()
            .unwrap()
            .contains("nothing-here")
    );
    assert_eq!(details["requests"][0]["ua"], "Mozilla/5.0");
    assert!(!details["trend"].as_array().unwrap().is_empty());

    // Players cannot read or act on alerts.
    let (s, _) = site.get("/admin/monitor/alerts", &pat).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Dismiss one with a note, ban the other for five minutes.
    let (s, b) = site
        .admin_call(
            "POST",
            &format!("/admin/monitor/alerts/{pat_alert}/dismiss"),
            Some(json!({ "note": "was testing the site" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        (b["status"].clone(), b["note"].clone()),
        (json!("dismissed"), json!("was testing the site"))
    );
    let (s, b) = site
        .admin_call(
            "POST",
            &format!("/admin/monitor/alerts/{lee_alert}/ban"),
            Some(json!({ "kind": "user", "duration_secs": 300 })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "banned");
    assert!(b["ban_id"].is_number());
    let (s, _) = site.get("/events", &lee).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "banned on the spot");
    // Already handled.
    let (s, _) = site
        .admin_call(
            "POST",
            &format!("/admin/monitor/alerts/{pat_alert}/dismiss"),
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // The same requests are not raised again; new behaviour after the dismissal is.
    let report = site.detect().await;
    assert_eq!(report.raised, 0, "{report:?}");
    site.advance(60);
    site.scan(&pat, "10.3.3.3", 30).await;
    assert_eq!(site.detect().await.raised, 1);
    assert_eq!(site.alerts("status=open").await["total"], 1);
}

#[tokio::test]
async fn the_organizer_tightens_or_loosens_every_detector() {
    let site = Site::new().await;
    let (pat, _) = site.player("pat").await;
    let (sam, _) = site.player("sam").await;
    let (s, list) = site
        .admin_call("GET", "/admin/monitor/detectors", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let scan = list["detectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["kind"] == "scan")
        .unwrap();
    assert_eq!(
        (
            scan["config"]["alert_at"].clone(),
            scan["customized"].clone()
        ),
        (json!(25), json!(false))
    );
    assert_eq!(list["detectors"].as_array().unwrap().len(), 6);

    // Loosened: never ban, alert only from 200.
    let loose = json!({ "enabled": true, "alert_at": 200, "ban_at": null, "window_secs": 120, "ban_secs": null });
    let (s, b) = site
        .admin_call("PUT", "/admin/monitor/detectors/scan", Some(loose))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    site.scan(&pat, "10.3.3.3", 130).await;
    let report = site.detect().await;
    assert_eq!(
        (report.raised, report.banned),
        (0, 0),
        "130 is below 200 now: {report:?}"
    );

    // Tightened: alert from 5, ban at 20, for an hour.
    let tight = json!({ "enabled": true, "alert_at": 5, "ban_at": 20, "window_secs": 120, "ban_secs": 3600 });
    site.admin_call("PUT", "/admin/monitor/detectors/scan", Some(tight))
        .await;
    let report = site.detect().await;
    assert_eq!(report.banned, 1, "{report:?}");
    let (_, bans) = site.admin_call("GET", "/admin/monitor/bans", None).await;
    assert_eq!(bans["bans"][0]["expires_at"], T0 + 3600);

    // Switched off, and put back.
    let off = json!({ "enabled": false, "alert_at": 5, "ban_at": null, "window_secs": 120, "ban_secs": null });
    site.admin_call("PUT", "/admin/monitor/detectors/scan", Some(off))
        .await;
    let (_, list) = site
        .admin_call("GET", "/admin/monitor/detectors", None)
        .await;
    let scan = list["detectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["kind"] == "scan")
        .unwrap();
    assert_eq!(
        (
            scan["config"]["enabled"].clone(),
            scan["customized"].clone()
        ),
        (json!(false), json!(true))
    );
    let (s, b) = site
        .admin_call("DELETE", "/admin/monitor/detectors/scan", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        b["detectors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["kind"] == "scan")
            .unwrap()["customized"],
        false
    );

    // Nonsense is refused, and so is a player.
    for bad in [
        json!({ "enabled": true, "alert_at": 0, "ban_at": null, "window_secs": 120, "ban_secs": null }),
        json!({ "enabled": true, "alert_at": 10, "ban_at": 5, "window_secs": 120, "ban_secs": null }),
        json!({ "enabled": true, "alert_at": 10, "ban_at": null, "window_secs": 1, "ban_secs": null }),
    ] {
        let (s, _) = site
            .admin_call("PUT", "/admin/monitor/detectors/scan", Some(bad))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    let (s, _) = site
        .admin_call("PUT", "/admin/monitor/detectors/nope", Some(json!({})))
        .await;
    assert!(
        s == StatusCode::NOT_FOUND || s == StatusCode::UNPROCESSABLE_ENTITY,
        "{s}"
    );
    let (s, _) = site
        .send(
            "PUT",
            "/admin/monitor/detectors/scan",
            Some(&sam),
            Some(json!({})),
            "10.3.3.3",
            "",
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_graphs_count_what_happened_minute_by_minute() {
    let site = Site::new().await;
    let (pat, _) = site.player("pat").await;
    // Minute 0: a failed and a good sign in, then browsing.
    site.send(
        "POST",
        "/auth/player-login",
        None,
        Some(json!({ "username": "pat", "password": "wrong" })),
        "10.6.6.6",
        "",
    )
    .await;
    for _ in 0..4 {
        site.get("/events", &pat).await;
    }
    // Minute 2: a burst, with a few unknown addresses.
    site.advance(120);
    for _ in 0..10 {
        site.get("/events", &pat).await;
    }
    site.scan(&pat, "10.3.3.3", 3).await;

    let (s, series) = site
        .admin_call("GET", "/admin/monitor/series?minutes=60", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{series}");
    assert_eq!(series["step"], 60);
    let buckets = series["buckets"].as_array().unwrap();
    assert!(
        buckets.len() >= 60,
        "empty minutes are present: {}",
        buckets.len()
    );
    let total: i64 = buckets.iter().map(|b| b["total"].as_i64().unwrap()).sum();
    // register + failed login + 4 + 10 + 3 scans + the organizer's own login.
    assert_eq!(total, 1 + 1 + 1 + 4 + 10 + 3, "{series}");
    let at = |t: i64| buckets.iter().find(|b| b["t"] == t).unwrap();
    let first = at((T0 / 60) * 60);
    assert_eq!(first["login_failed"], 1);
    assert_eq!(first["signups"], 1);
    let burst = at(((T0 + 120) / 60) * 60);
    assert_eq!(burst["total"], 13);
    assert_eq!(
        burst["client"], 3,
        "the unknown addresses are client errors"
    );
    assert_eq!(burst["users"], 1);
    assert!(burst["max_ms"].as_i64().unwrap() >= 0);

    // Alerts appear on the graph as markers once raised.
    site.detect().await;
    let (_, with_alert) = site
        .admin_call("GET", "/admin/monitor/series?minutes=60", None)
        .await;
    assert!(
        with_alert["markers"].as_array().unwrap().is_empty(),
        "3 unknown addresses are not an alert"
    );
    site.scan(&pat, "10.3.3.3", 30).await;
    site.detect().await;
    let (_, with_alert) = site
        .admin_call("GET", "/admin/monitor/series?minutes=60", None)
        .await;
    assert_eq!(with_alert["markers"].as_array().unwrap().len(), 1);

    let (s, hot) = site
        .admin_call("GET", "/admin/monitor/hot?minutes=5", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{hot}");
    assert_eq!(hot["players"][0]["label"], "pat");
    assert_eq!(hot["players"][0]["trend"].as_array().unwrap().len(), 5);
    assert!(hot["addresses"].as_array().unwrap().len() >= 2);

    let (s, heat) = site.admin_call("GET", "/admin/monitor/heatmap", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(heat["hours"].as_array().unwrap().len(), 168);
    assert!(
        heat["hours"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_i64().unwrap())
            .sum::<i64>()
            >= 50
    );

    // Staff only.
    let (s, _) = site.get("/admin/monitor/series", &pat).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = site
        .send("GET", "/admin/monitor/series", None, None, "10.3.3.3", "")
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}
