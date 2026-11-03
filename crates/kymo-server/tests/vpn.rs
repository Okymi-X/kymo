use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use kymo_instances::{FakeRuntime, Policy, ResourceCaps};
use kymo_server::state::InstanceSetup;
use kymo_server::vpn::VpnSetup;
use kymo_server::{AppState, db, instances, router, vpn};
use kymo_vpn::{FakeGateway, GatewayConfig, LabPool, TunnelPool};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    bytes: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

struct Lab {
    app: Router,
    state: AppState,
    clock: Arc<AtomicI64>,
    runtime: Arc<FakeRuntime>,
    gateway: Arc<FakeGateway>,
    dir: tempfile::TempDir,
    admin: String,
    event_id: i64,
    slug: String,
    category: i64,
}

fn policy() -> Policy {
    Policy {
        restart_cooldown_secs: 0,
        max_starts: 20,
        max_concurrent: 1,
    }
}

impl Lab {
    /// A server with instances, optionally with the VPN, and one Jeopardy event.
    async fn new(with_vpn: bool, mode: &str) -> Lab {
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let clock = Arc::new(AtomicI64::new(T0));
        let shared = Arc::clone(&clock);
        let runtime = Arc::new(FakeRuntime::new());
        let gateway = Arc::new(FakeGateway::new());
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new(pool, PASSWORD)
            .unwrap()
            .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)))
            .with_instances(InstanceSetup {
                runtime: runtime.clone(),
                policy: policy(),
                caps: ResourceCaps::default(),
                public_host: "play.example.org".into(),
                reap_grace_secs: 5,
            });
        if with_vpn {
            state = state.with_vpn(VpnSetup {
                gateway: gateway.clone(),
                runtime: runtime.clone(),
                dir: dir.path().to_path_buf(),
                gateway_config: GatewayConfig {
                    name: "kymo-vpn-gateway".into(),
                    image: "img".into(),
                    dir: dir.path().to_path_buf(),
                    publish_ip: "0.0.0.0".into(),
                    publish_port: 1194,
                    bind_options: String::new(),
                },
                public_host: "vpn.example.org".into(),
                public_port: 1194,
                tunnel: TunnelPool::new("10.8.0.0/16".parse().unwrap()).unwrap(),
                labs: LabPool::new("10.77.0.0/16".parse().unwrap(), 27).unwrap(),
                client_days: 60,
                lock: Arc::default(),
                local: true,
            });
        }
        let app = router(state.clone());
        let mut lab = Lab {
            app,
            state,
            clock,
            runtime,
            gateway,
            dir,
            admin: String::new(),
            event_id: 0,
            slug: String::new(),
            category: 0,
        };
        lab.admin = lab.login().await;
        let (_, event) = lab
            .call(
                "POST",
                "/admin/events",
                Some(&lab.admin.clone()),
                Some(json!({ "name": "Lab CTF", "kind": "jeopardy", "mode": mode })),
            )
            .await;
        lab.event_id = event["id"].as_i64().unwrap();
        lab.slug = event["slug"].as_str().unwrap().to_owned();
        let admin = lab.admin.clone();
        let (_, cat) = lab
            .call(
                "POST",
                &format!("/admin/events/{}/categories", lab.event_id),
                Some(&admin),
                Some(json!({ "name": "Web" })),
            )
            .await;
        lab.category = cat["id"].as_i64().unwrap();
        lab.set_state("registration_open").await;
        lab
    }

    fn set_time(&self, t: i64) {
        self.clock.store(t, Ordering::SeqCst);
    }

    async fn raw(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Reply {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"));
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let bytes = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                v.to_string().into_bytes()
            }
            None => Vec::new(),
        };
        let addr: SocketAddr = "10.0.0.1:4000".parse().unwrap();
        let res = self
            .app
            .clone()
            .oneshot(
                req.extension(ConnectInfo(addr))
                    .body(Body::from(bytes))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        Reply {
            status,
            headers,
            bytes: res.into_body().collect().await.unwrap().to_bytes().to_vec(),
        }
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let r = self.raw(method, uri, token, body).await;
        (r.status, r.json())
    }

    async fn login(&self) -> String {
        let (_, b) = self
            .call(
                "POST",
                "/auth/login",
                None,
                Some(json!({ "password": PASSWORD })),
            )
            .await;
        b["token"].as_str().unwrap().to_owned()
    }

    async fn admin(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call(method, uri, Some(&self.admin), body).await
    }

    async fn set_state(&self, state: &str) {
        let (s, b) = self
            .admin(
                "POST",
                &format!("/admin/events/{}/state", self.event_id),
                Some(json!({ "state": state })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }

    /// A registered player. In team mode the player first makes or joins `team`.
    async fn player(&self, name: &str) -> (String, i64) {
        let (s, b) = self
            .call(
                "POST",
                "/auth/register",
                None,
                Some(json!({ "username": name, "password": PLAYER_PASSWORD })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        (
            b["token"].as_str().unwrap().to_owned(),
            b["user"]["id"].as_i64().unwrap(),
        )
    }

    async fn register(&self, token: &str) {
        let (s, b) = self
            .call(
                "POST",
                &format!("/events/{}/register", self.slug),
                Some(token),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
    }

    async fn challenge(&self, title: &str) -> i64 {
        let body = json!({
            "category_id": self.category, "title": title, "description": "x",
            "scoring": { "kind": "static", "points": 100 }, "flag": { "kind": "static", "value": "KYMO{x}" },
            "requires_instance": true,
            "instance_config": { "image": "ctf/web:1", "port": 8000, "cpu_millis": 250, "memory_mb": 64, "pids": 64, "ttl_secs": 600 },
        });
        let (s, c) = self
            .admin(
                "POST",
                &format!("/admin/events/{}/challenges", self.event_id),
                Some(body),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{c}");
        let id = c["id"].as_i64().unwrap();
        self.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
            .await;
        id
    }

    async fn network(&self, mode: &str) -> (StatusCode, Value) {
        self.admin(
            "PUT",
            &format!("/admin/events/{}/network", self.event_id),
            Some(json!({ "mode": mode })),
        )
        .await
    }

    async fn act(
        &self,
        token: &str,
        method: &str,
        challenge: i64,
        tail: &str,
    ) -> (StatusCode, Value) {
        self.call(
            method,
            &format!(
                "/events/{}/challenges/{challenge}/instance{tail}",
                self.slug
            ),
            Some(token),
            None,
        )
        .await
    }

    async fn profile(&self, token: &str) -> Reply {
        self.raw("POST", "/vpn/profile", Some(token), None).await
    }

    fn file(&self, path: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(path)).unwrap_or_default()
    }
}

#[tokio::test]
async fn a_server_without_the_vpn_says_so_and_refuses_vpn_events() {
    let lab = Lab::new(false, "solo").await;
    let (token, _) = lab.player("pat").await;
    let (s, b) = lab.call("GET", "/vpn", Some(&token), None).await;
    assert_eq!((s, b["enabled"].clone()), (StatusCode::OK, json!(false)));
    assert_eq!(
        lab.profile(&token).await.status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let (s, b) = lab.network("vpn").await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (_, event) = lab
        .call("GET", &format!("/events/{}", lab.slug), None, None)
        .await;
    assert_eq!(event["network_mode"], "published");
    let (_, admin) = lab.admin("GET", "/admin/vpn", None).await;
    assert_eq!(admin["enabled"], false);
}

#[tokio::test]
async fn a_player_downloads_a_profile_and_a_new_one_replaces_the_old() {
    let lab = Lab::new(true, "solo").await;
    let (token, id) = lab.player("pat").await;
    let (_, status) = lab.call("GET", "/vpn", Some(&token), None).await;
    assert_eq!(
        (status["enabled"].clone(), status["has_profile"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(
        lab.call("GET", "/vpn", None, None).await.0,
        StatusCode::UNAUTHORIZED
    );

    let first = lab.profile(&token).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(
        first.headers["content-type"],
        "application/x-openvpn-profile"
    );
    assert_eq!(first.headers["cache-control"], "no-store");
    assert_eq!(
        first.headers["content-disposition"],
        "attachment; filename=\"kymo.ovpn\""
    );
    let text = first.text();
    for needle in [
        "remote vpn.example.org 1194",
        "<ca>",
        "<cert>",
        "<key>",
        "<tls-crypt>",
        "remote-cert-tls server",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
    assert_eq!(first.headers["x-kymo-tunnel-ip"], "10.8.0.2");
    assert_eq!(
        lab.file(&format!("ccd/u{id}")),
        "ifconfig-push 10.8.0.2 255.255.0.0\n"
    );
    for f in [
        "pki/ca.crt",
        "pki/server.crt",
        "pki/server.key",
        "pki/ta.key",
        "pki/crl.pem",
        "server.conf",
        "rules.sh",
    ] {
        assert!(!lab.file(f).is_empty(), "{f} is written");
    }
    assert!(lab.file("server.conf").contains("ccd-exclusive"));
    assert!(
        !lab.file("pki/crl.pem").contains("serial"),
        "an empty list so far"
    );
    assert!(
        !lab.file("pki/ca.crt").contains("PRIVATE KEY"),
        "the CA key never leaves the database"
    );
    assert!(!lab.file("pki/server.crt").contains("PRIVATE KEY"));

    let second = lab.profile(&token).await;
    assert_eq!(
        second.headers["x-kymo-tunnel-ip"], "10.8.0.2",
        "the address stays the same"
    );
    assert_ne!(first.text(), second.text(), "a new certificate every time");
    let revoked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vpn_revoked")
        .fetch_one(&lab.state.pool)
        .await
        .unwrap();
    assert_eq!(revoked, 1, "the first certificate is revoked");
    assert!(lab.file("pki/crl.pem").contains("BEGIN X509 CRL"));

    let (other, other_id) = lab.player("sam").await;
    let third = lab.profile(&other).await;
    assert_eq!(third.headers["x-kymo-tunnel-ip"], "10.8.0.3");
    assert!(lab.file(&format!("ccd/u{other_id}")).contains("10.8.0.3"));
    let (_, mine) = lab.call("GET", "/vpn", Some(&token), None).await;
    assert_eq!(
        (mine["has_profile"].clone(), mine["tunnel_ip"].clone()),
        (json!(true), json!("10.8.0.2"))
    );
    assert!(lab.gateway.rules_runs() >= 3);
}

#[tokio::test]
async fn a_banned_or_revoked_player_loses_their_file_and_their_session() {
    let lab = Lab::new(true, "solo").await;
    let (token, id) = lab.player("pat").await;
    let (keep, keep_id) = lab.player("sam").await;
    lab.profile(&token).await;
    lab.profile(&keep).await;
    assert!(!lab.file(&format!("ccd/u{id}")).is_empty());

    let (s, _) = lab
        .admin(
            "PUT",
            &format!("/admin/users/{id}"),
            Some(json!({ "banned": true })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        lab.file(&format!("ccd/u{id}")),
        "",
        "the ban removes the address file"
    );
    assert!(
        lab.gateway.kicked().contains(&format!("u{id}")),
        "and drops the live session"
    );
    assert!(
        !lab.file(&format!("ccd/u{keep_id}")).is_empty(),
        "others are untouched"
    );

    // Revoking by hand does the same and forgets the identity.
    let uri = format!("/admin/vpn/clients/{keep_id}");
    assert_eq!(
        lab.admin("DELETE", &uri, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(lab.file(&format!("ccd/u{keep_id}")), "");
    assert!(lab.gateway.kicked().contains(&format!("u{keep_id}")));
    assert_eq!(
        lab.admin("DELETE", &uri, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        lab.call("DELETE", &uri, Some(&keep), None).await.0,
        StatusCode::FORBIDDEN
    );
    let (_, audit) = lab
        .admin("GET", "/admin/monitor/audit?action=vpn_revoke", None)
        .await;
    assert_eq!(audit["total"], 1);
}

#[tokio::test]
async fn a_banned_player_cannot_get_a_new_profile() {
    let lab = Lab::new(true, "solo").await;
    let (token, id) = lab.player("pat").await;
    lab.admin(
        "PUT",
        &format!("/admin/users/{id}"),
        Some(json!({ "banned": true })),
    )
    .await;
    assert!(matches!(
        lab.profile(&token).await.status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ));
}

#[tokio::test]
async fn vpn_instances_get_a_private_lab_address_and_nothing_is_published() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box A").await;
    let b = lab.challenge("Box B").await;
    let (pat, pat_id) = lab.player("pat").await;
    let (sam, _) = lab.player("sam").await;
    lab.register(&pat).await;
    lab.register(&sam).await;
    lab.profile(&pat).await;
    lab.profile(&sam).await;
    lab.set_state("live").await;
    assert_eq!(lab.network("vpn").await.0, StatusCode::OK);

    let (s, started) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED, "{started}");
    assert_eq!(started["network"], "vpn");
    assert_eq!(
        started["host"], "10.77.0.10",
        "the first lab, the first instance slot"
    );
    assert_eq!(started["port"], 8000, "the service port, nothing published");
    assert!(!started.to_string().contains("play.example.org"));
    assert_eq!(
        lab.runtime.labs(),
        vec![("kymo-lab-0".to_owned(), "10.77.0.0/27".to_owned())]
    );
    assert_eq!(lab.gateway.networks(), vec!["kymo-lab-0".to_owned()]);
    let rules = lab.file("rules.sh");
    assert!(
        rules.contains("-s 10.8.0.2 -d 10.77.0.0/27 -j ACCEPT"),
        "{rules}"
    );
    assert!(!rules.contains("10.8.0.3"), "sam has no lab yet");

    // Sam gets a different lab, and the two are not allowed to reach each other.
    let (s, theirs) = lab.act(&sam, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED, "{theirs}");
    assert_eq!(theirs["host"], "10.77.0.42");
    let rules = lab.file("rules.sh");
    assert!(rules.contains("-s 10.8.0.3 -d 10.77.0.32/27 -j ACCEPT"));
    assert!(
        !rules.contains("-s 10.8.0.2 -d 10.77.0.32/27"),
        "no cross access"
    );
    assert!(
        !rules.contains("-s 10.8.0.3 -d 10.77.0.0/27"),
        "no cross access"
    );

    // The same player's next challenge shares their lab; a restart takes the next address.
    lab.act(&pat, "DELETE", a, "").await;
    let (_, again) = lab.act(&pat, "POST", b, "").await;
    assert_eq!(
        again["host"], "10.77.0.11",
        "addresses are handed out in turn: the next one, not the one that just ended"
    );
    let (_, restarted) = lab.act(&pat, "POST", b, "/restart").await;
    assert_eq!(
        restarted["host"], "10.77.0.12",
        "a restart moves on again, so a stale connection cannot hit the new service"
    );
    assert_eq!(
        lab.runtime.labs().len(),
        2,
        "one lab per player, not per challenge"
    );
    let lab_row: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vpn_lab WHERE scorer_id = ?")
        .bind(pat_id)
        .fetch_one(&lab.state.pool)
        .await
        .unwrap();
    assert_eq!(lab_row, 1);
}

#[tokio::test]
async fn the_network_mode_cannot_change_under_running_instances() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    assert_eq!(lab.network("vpn").await.0, StatusCode::OK);
    let (s, _) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, b) = lab.network("published").await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    lab.act(&pat, "DELETE", a, "").await;
    assert_eq!(lab.network("published").await.0, StatusCode::OK);
    let (s, _) = lab
        .admin(
            "PUT",
            &format!("/admin/events/{}/network", lab.event_id),
            Some(json!({ "mode": "nonsense" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (player, _) = lab.player("sam").await;
    assert_eq!(
        lab.call(
            "PUT",
            &format!("/admin/events/{}/network", lab.event_id),
            Some(&player),
            Some(json!({ "mode": "vpn" }))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn published_events_still_publish_a_port_even_when_the_vpn_is_on() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    let (s, started) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED, "{started}");
    assert_eq!(started["network"], "published");
    assert_eq!(started["host"], "play.example.org");
    assert!(lab.runtime.labs().is_empty());
}

#[tokio::test]
async fn a_vpn_event_needs_the_vpn_to_start_and_failures_are_free() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    lab.network("vpn").await;

    lab.gateway.set_fail(true);
    let (s, b) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{b}");
    assert!(b["error"].as_str().unwrap().contains("VPN gateway"));
    assert_eq!(lab.runtime.running_count(), 0);
    lab.gateway.set_fail(false);
    let (s, b) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(
        s,
        StatusCode::CREATED,
        "a failed start did not use the budget: {b}"
    );
    let (_, again) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(again["starts_used"], 1);
}

#[tokio::test]
async fn team_members_share_a_lab_and_leaving_closes_it_to_them() {
    let lab = Lab::new(true, "team").await;
    let a = lab.challenge("Box").await;
    let (owner, owner_id) = lab.player("owner").await;
    let (mate, mate_id) = lab.player("mate").await;
    let (outsider, _) = lab.player("outsider").await;
    let (s, team) = lab
        .call(
            "POST",
            "/teams",
            Some(&owner),
            Some(json!({ "name": "Crew" })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{team}");
    let code = team["invite_code"].as_str().unwrap().to_owned();
    let (s, _) = lab
        .call(
            "POST",
            "/teams/join",
            Some(&mate),
            Some(json!({ "invite_code": code })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    lab.profile(&owner).await;
    lab.profile(&mate).await;
    lab.profile(&outsider).await;
    lab.register(&owner).await;
    lab.set_state("live").await;
    lab.network("vpn").await;

    let (s, started) = lab.act(&owner, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED, "{started}");
    let rules = lab.file("rules.sh");
    assert!(
        rules.contains("-s 10.8.0.2 -d 10.77.0.0/27 -j ACCEPT"),
        "the owner: {rules}"
    );
    assert!(
        rules.contains("-s 10.8.0.3 -d 10.77.0.0/27 -j ACCEPT"),
        "the teammate: {rules}"
    );
    assert!(!rules.contains("10.8.0.4"), "not the outsider");
    let _ = (owner_id, mate_id);

    // Rosters lock when an event starts, so let this one change.
    let (s, _) = lab
        .admin(
            "PUT",
            &format!("/admin/events/{}", lab.event_id),
            Some(json!({ "roster_locked": false })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = lab
        .call(
            "POST",
            &format!("/teams/{}/leave", team["id"]),
            Some(&mate),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let rules = lab.file("rules.sh");
    assert!(
        !rules.contains("-s 10.8.0.3"),
        "a member who left may no longer reach the lab: {rules}"
    );
    assert!(rules.contains("-s 10.8.0.2 -d 10.77.0.0/27"));
}

#[tokio::test]
async fn idle_labs_are_removed_and_the_firewall_closes_with_them() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.profile(&pat).await;
    lab.set_state("live").await;
    lab.network("vpn").await;
    lab.act(&pat, "POST", a, "").await;
    assert_eq!(lab.runtime.labs().len(), 1);

    // While an instance runs the lab stays, however long it has been.
    lab.set_time(T0 + 300);
    assert_eq!(vpn::collect_idle_labs(&lab.state).await.unwrap(), 0);
    lab.act(&pat, "DELETE", a, "").await;
    lab.set_time(T0 + 400);
    assert_eq!(
        vpn::collect_idle_labs(&lab.state).await.unwrap(),
        0,
        "kept for a while for the next start"
    );
    lab.set_time(T0 + 300 + vpn::IDLE_LAB_SECS + 1);
    assert_eq!(vpn::collect_idle_labs(&lab.state).await.unwrap(), 1);
    assert!(lab.runtime.labs().is_empty());
    assert!(lab.gateway.networks().is_empty());
    assert!(
        !lab.file("rules.sh").contains("ACCEPT"),
        "{}",
        lab.file("rules.sh")
    );
    let again = lab.act(&pat, "POST", a, "").await;
    assert_eq!(again.0, StatusCode::CREATED, "a new lab is made on demand");
    assert_eq!(lab.runtime.labs().len(), 1);
}

#[tokio::test]
async fn an_ended_event_releases_its_labs_through_the_reaper() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.profile(&pat).await;
    lab.set_state("live").await;
    lab.network("vpn").await;
    lab.act(&pat, "POST", a, "").await;
    lab.set_state("ended").await;
    // The reaper stops the instance because the event is over, then the empty lab goes.
    let ended = instances::reap(&lab.state).await.unwrap();
    assert!(ended >= 1);
    assert_eq!(lab.runtime.running_count(), 0);
    assert!(lab.runtime.labs().is_empty(), "{:?}", lab.runtime.labs());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vpn_lab")
        .fetch_one(&lab.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn staff_see_the_gateway_and_players_do_not() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.profile(&pat).await;
    lab.set_state("live").await;
    lab.network("vpn").await;
    lab.act(&pat, "POST", a, "").await;

    let (s, view) = lab.admin("GET", "/admin/vpn", None).await;
    assert_eq!(s, StatusCode::OK, "{view}");
    assert_eq!(view["gateway"]["running"], true);
    assert_eq!(view["clients"], 1);
    assert_eq!(view["lab_pool"]["used"], 1);
    assert_eq!(view["lab_pool"]["capacity"], 2048);
    assert_eq!(view["labs"][0]["subnet"], "10.77.0.0/27");
    assert_eq!(view["labs"][0]["live_instances"], 1);
    assert!(!view.to_string().contains("PRIVATE KEY"));
    assert_eq!(
        lab.call("GET", "/admin/vpn", Some(&pat), None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        lab.call("GET", "/admin/vpn", None, None).await.0,
        StatusCode::UNAUTHORIZED
    );

    let (s, report) = lab.admin("POST", "/admin/vpn/sync", None).await;
    assert_eq!(s, StatusCode::OK, "{report}");
    assert_eq!(
        (
            report["labs"].clone(),
            report["clients"].clone(),
            report["allows"].clone()
        ),
        (json!(1), json!(1), json!(1))
    );

    let (_, health) = lab.admin("GET", "/admin/ops/health", None).await;
    assert_eq!(health["vpn"]["enabled"], true);
    assert_eq!(health["vpn"]["gateway_ok"], true);
    assert_eq!(health["status"], "ok");
    lab.gateway.set_fail(true);
    let (_, health) = lab.admin("GET", "/admin/ops/health", None).await;
    assert_eq!(health["vpn"]["gateway_ok"], false);
    assert_eq!(
        health["status"], "degraded",
        "an unreachable gateway is a degraded server"
    );
}

#[tokio::test]
async fn sync_heals_a_lost_gateway_and_the_authority_survives_restarts() {
    let lab = Lab::new(true, "solo").await;
    let (pat, _) = lab.player("pat").await;
    let first = lab.profile(&pat).await.text();
    let ca = |t: &str| {
        t.split("<ca>")
            .nth(1)
            .unwrap()
            .split("</ca>")
            .next()
            .unwrap()
            .to_owned()
    };
    // A second download comes from the same authority, so old profiles keep trusting the gateway.
    assert_eq!(ca(&first), ca(&lab.profile(&pat).await.text()));
    let before = lab.file("pki/server.crt");
    vpn::sync(&lab.state).await.unwrap();
    assert_eq!(
        before,
        lab.file("pki/server.crt"),
        "the gateway certificate is kept, not reissued"
    );
    // Files removed behind our back are written again.
    std::fs::remove_file(lab.dir.path().join("rules.sh")).unwrap();
    std::fs::remove_dir_all(lab.dir.path().join("ccd")).unwrap();
    vpn::sync(&lab.state).await.unwrap();
    assert!(!lab.file("rules.sh").is_empty());
    assert!(lab.file("ccd/u1").contains("ifconfig-push"));
}

#[tokio::test]
async fn event_bundles_carry_the_network_mode_and_respect_the_server() {
    let lab = Lab::new(true, "solo").await;
    lab.network("vpn").await;
    let export = lab
        .raw(
            "GET",
            &format!("/admin/events/{}/export", lab.event_id),
            Some(&lab.admin),
            None,
        )
        .await;
    assert_eq!(export.status, StatusCode::OK);
    assert_eq!(export.json()["event"]["network_mode"], "vpn");
    let imported = lab
        .raw(
            "POST",
            "/admin/events/import",
            Some(&lab.admin),
            Some(export.json()),
        )
        .await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.text());
    assert_eq!(imported.json()["network_mode"], "vpn");

    // A server with no VPN imports the same bundle as a normal published event.
    let plain = Lab::new(false, "solo").await;
    let imported = plain
        .raw(
            "POST",
            "/admin/events/import",
            Some(&plain.admin),
            Some(export.json()),
        )
        .await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.text());
    assert_eq!(imported.json()["network_mode"], "published");
    // Bundles from before the network mode existed still import.
    let mut old = export.json();
    old["event"].as_object_mut().unwrap().remove("network_mode");
    let imported = plain
        .raw(
            "POST",
            "/admin/events/import",
            Some(&plain.admin),
            Some(old),
        )
        .await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.text());
}

#[tokio::test]
async fn the_profile_endpoint_is_rate_limited_by_a_seeded_rule() {
    let lab = Lab::new(true, "solo").await;
    let (_, rules) = lab.admin("GET", "/admin/monitor/rules", None).await;
    let endpoints: Vec<&str> = rules["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["endpoint"].as_str().unwrap())
        .collect();
    assert!(
        endpoints.contains(&"POST /api/v1/vpn/profile"),
        "{endpoints:?}"
    );
    let (pat, _) = lab.player("pat").await;
    for _ in 0..10 {
        assert_eq!(lab.profile(&pat).await.status, StatusCode::OK);
    }
    assert_eq!(
        lab.profile(&pat).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn an_address_is_shown_only_once_the_service_answers_and_never_after_it_dies() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    lab.network("vpn").await;

    // The container exists but nothing listens yet: no address, no port, state starting.
    lab.gateway.set_unreachable(true);
    let (s, started) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED, "{started}");
    assert_eq!(started["state"], "starting");
    assert_eq!(started["ready"], false);
    assert!(
        started["host"].is_null() && started["port"].is_null(),
        "{started}"
    );
    let (_, polled) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(polled["state"], "starting", "still not answering");
    assert!(polled["host"].is_null());

    // It starts answering: the next look flips it to running and reveals the address.
    lab.gateway.set_unreachable(false);
    let (_, ready) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(
        (ready["state"].clone(), ready["ready"].clone()),
        (json!("running"), json!(true))
    );
    assert_eq!(ready["host"], "10.77.0.10");
    assert_eq!(ready["port"], 8000);

    // The container dies on its own: the address disappears and the reason is given.
    let handle = lab.runtime.list_handles()[0].clone();
    lab.runtime.set_exited(&handle);
    let (_, dead) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(dead["state"], "failed", "{dead}");
    assert_eq!(dead["failure"], "exited");
    assert!(dead["host"].is_null() && dead["port"].is_null());
    assert_eq!(
        lab.runtime.running_count(),
        0,
        "the dead container is cleaned up"
    );
    let (_, again) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(
        again["starts_used"], 0,
        "a crash does not use the start budget"
    );
    let (s, restarted) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED, "{restarted}");
    assert_eq!(
        restarted["host"], "10.77.0.11",
        "and the new run gets a fresh address"
    );
}

#[tokio::test]
async fn a_service_that_never_answers_fails_after_the_timeout_and_frees_the_player() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    lab.network("vpn").await;
    lab.gateway.set_unreachable(true);
    lab.act(&pat, "POST", a, "").await;

    lab.set_time(T0 + instances::READY_TIMEOUT_SECS - 1);
    let (_, waiting) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(waiting["state"], "starting");
    lab.set_time(T0 + instances::READY_TIMEOUT_SECS + 1);
    let (_, failed) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(failed["state"], "failed", "{failed}");
    assert_eq!(failed["failure"], "never_ready");
    assert!(failed["host"].is_null());
    assert_eq!(
        lab.runtime.running_count(),
        0,
        "the container that never answered is removed"
    );
    assert_eq!(failed["starts_used"], 0);
    lab.gateway.set_unreachable(false);
    let (s, _) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(s, StatusCode::CREATED, "the player can try again at once");
}

#[tokio::test]
async fn the_reaper_also_settles_starting_and_crashed_instances() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    lab.network("vpn").await;
    lab.act(&pat, "POST", a, "").await;
    let handle = lab.runtime.list_handles()[0].clone();
    lab.runtime.set_exited(&handle);
    // Nobody is looking at the page, yet the reaper notices.
    instances::reap(&lab.state).await.unwrap();
    let (_, view) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(
        (view["state"].clone(), view["failure"].clone()),
        (json!("failed"), json!("exited"))
    );
    assert_eq!(lab.runtime.running_count(), 0);
}

#[tokio::test]
async fn a_player_sees_their_own_instances_with_the_challenge_each_belongs_to() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Alpha").await;
    let (pat, _) = lab.player("pat").await;
    let (sam, _) = lab.player("sam").await;
    lab.register(&pat).await;
    lab.register(&sam).await;
    lab.set_state("live").await;
    lab.network("vpn").await;
    let uri = format!("/events/{}/instances", lab.slug);

    let (_, none) = lab.call("GET", &uri, Some(&pat), None).await;
    assert_eq!(none["instances"].as_array().unwrap().len(), 0);
    lab.act(&pat, "POST", a, "").await;
    lab.act(&sam, "POST", a, "").await;
    let (s, mine) = lab.call("GET", &uri, Some(&pat), None).await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    let list = mine["instances"].as_array().unwrap();
    assert_eq!(list.len(), 1, "only their own");
    assert_eq!(
        (
            list[0]["title"].clone(),
            list[0]["host"].clone(),
            list[0]["network"].clone()
        ),
        (json!("Alpha"), json!("10.77.0.10"), json!("vpn"))
    );
    let (_, theirs) = lab.call("GET", &uri, Some(&sam), None).await;
    assert_eq!(
        theirs["instances"][0]["host"], "10.77.0.42",
        "another lab, another range"
    );
    assert_eq!(
        lab.call("GET", &uri, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );

    lab.act(&pat, "DELETE", a, "").await;
    let (_, after) = lab.call("GET", &uri, Some(&pat), None).await;
    assert_eq!(
        after["instances"].as_array().unwrap().len(),
        0,
        "stopped instances leave the list"
    );
}

// ---------- a lab host with its own gateway ----------

const LAB_TOKEN: &str = "lab-test-token-with-enough-length-1";

/// A lab host with a gateway of its own, served over a real socket, with everything it drives
/// kept in memory so the test can look at it.
struct RemoteLab {
    url: String,
    runtime: Arc<FakeRuntime>,
    gateway: Arc<FakeGateway>,
    dir: tempfile::TempDir,
}

async fn remote_lab() -> RemoteLab {
    let manifest: kymo_lab::Manifest = serde_json::from_value(json!({
        "artifact": "ctf/web:1", "port": 8000, "env_allow": [], "max_ttl_secs": 600,
    }))
    .unwrap();
    let mut cfg = kymo_lab::server::LabConfig::new("remote", LAB_TOKEN);
    cfg.manifests = vec![manifest];
    let runtime = Arc::new(FakeRuntime::new());
    let lab = kymo_lab::server::Lab::new(cfg, runtime.clone());
    let gateway = Arc::new(FakeGateway::new());
    let dir = tempfile::tempdir().unwrap();
    lab.set_vpn(
        Arc::new(kymo_lab::vpn::LabVpn::new(
            gateway.clone(),
            runtime.clone(),
            GatewayConfig {
                name: "kymo-lab-vpn-gateway".into(),
                image: "img".into(),
                dir: dir.path().to_path_buf(),
                publish_ip: "0.0.0.0".into(),
                publish_port: 1194,
                bind_options: String::new(),
            },
            "lab.example.org".into(),
            "10.77.0.0/16".parse().unwrap(),
        )),
        true,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(kymo_lab::server::serve(
        listener,
        kymo_lab::server::router(lab),
        None,
        std::future::pending(),
    ));
    RemoteLab {
        url,
        runtime,
        gateway,
        dir,
    }
}

impl Lab {
    /// Registers the remote lab host, gives the event to it and switches the event to the VPN.
    async fn use_remote(&self, remote: &RemoteLab) -> i64 {
        let (s, added) = self
            .admin(
                "POST",
                "/admin/labs",
                Some(json!({
                    "name": "Remote", "api_url": remote.url, "token": LAB_TOKEN,
                    "public_host": "203.0.113.9",
                })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{added}");
        let id = added["lab"]["id"].as_i64().unwrap();
        let (s, b) = self
            .admin(
                "PUT",
                &format!("/admin/events/{}/lab", self.event_id),
                Some(json!({ "lab_host_id": id, "network_mode": "vpn" })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["network_mode"], "vpn");
        id
    }
}

#[tokio::test]
async fn a_vpn_event_on_a_lab_host_uses_that_hosts_gateway_and_publishes_nothing() {
    let lab = Lab::new(true, "solo").await;
    let remote = remote_lab().await;
    let challenge = lab.challenge("Box A").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    let host_id = lab.use_remote(&remote).await;

    // The profile points at the lab host's gateway, not this server's.
    let profile = lab
        .raw(
            "POST",
            &format!("/vpn/profile?lab_host={host_id}"),
            Some(&pat),
            None,
        )
        .await;
    assert_eq!(profile.status, StatusCode::OK, "{}", profile.text());
    let text = profile.text();
    assert!(text.contains("remote lab.example.org 1194"), "{text}");
    assert!(!text.contains("vpn.example.org"));
    // That host's gateway knows the player; this server's own gateway has not been touched.
    assert!(remote.dir.path().join("ccd/u1").exists());
    assert_eq!(lab.gateway.networks(), Vec::<String>::new());

    let (s, started) = lab.act(&pat, "POST", challenge, "").await;
    assert_eq!(s, StatusCode::CREATED, "{started}");
    assert_eq!(started["network"], "vpn");
    assert_eq!(started["host"], "10.77.0.10");
    assert_eq!(started["port"], 8000);
    assert!(!started.to_string().contains("203.0.113.9"));

    // The lab network, the placement and the gateway attachment are all on the lab host.
    assert_eq!(
        remote.runtime.labs(),
        vec![("kymo-lab-0".to_owned(), "10.77.0.0/27".to_owned())]
    );
    assert_eq!(remote.gateway.networks(), vec!["kymo-lab-0".to_owned()]);
    assert!(lab.runtime.labs().is_empty(), "nothing was made locally");
    assert_eq!(remote.runtime.running_count(), 1);
    let rules = std::fs::read_to_string(remote.dir.path().join("rules.sh")).unwrap();
    assert!(
        rules.contains("-s 10.8.0.2 -d 10.77.0.0/27 -j ACCEPT"),
        "{rules}"
    );

    // Stopping and then waiting past the idle time takes the lab down on the lab host too.
    lab.act(&pat, "DELETE", challenge, "").await;
    lab.set_time(T0 + vpn::IDLE_LAB_SECS + 60);
    instances::reap(&lab.state).await.unwrap();
    assert!(remote.runtime.labs().is_empty());
    assert!(remote.gateway.networks().is_empty());
}

#[tokio::test]
async fn certificates_are_per_host_so_one_hosts_profile_opens_nothing_on_another() {
    let lab = Lab::new(true, "solo").await;
    let remote = remote_lab().await;
    let (pat, _) = lab.player("pat").await;
    let host_id = lab.use_remote(&remote).await;
    let local = lab.profile(&pat).await.text();
    let first = lab
        .raw(
            "POST",
            &format!("/vpn/profile?lab_host={host_id}"),
            Some(&pat),
            None,
        )
        .await
        .text();
    assert_ne!(first, local);
    // Different authorities: a different CA for each host.
    let ca = |t: &str| {
        t.split("<ca>")
            .nth(1)
            .and_then(|r| r.split("</ca>").next())
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    assert_ne!(ca(&first), ca(&local));
    // A new profile for the lab host replaces only that host's: the local one stays valid.
    let serials = |hid: i64| {
        let pool = lab.state.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT serial FROM vpn_client WHERE lab_host_id = ? AND user_id = 1",
            )
            .bind(hid)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let (local_before, remote_before) = (serials(0).await, serials(host_id).await);
    lab.raw(
        "POST",
        &format!("/vpn/profile?lab_host={host_id}"),
        Some(&pat),
        None,
    )
    .await;
    assert_eq!(serials(0).await, local_before);
    assert_ne!(serials(host_id).await, remote_before);
    let revoked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vpn_revoked WHERE lab_host_id = 0")
        .fetch_one(&lab.state.pool)
        .await
        .unwrap();
    assert_eq!(revoked, 0, "nothing was revoked on this server's gateway");
}

#[tokio::test]
async fn an_event_cannot_use_the_vpn_on_a_lab_host_that_has_no_gateway() {
    let lab = Lab::new(true, "solo").await;
    let manifest: kymo_lab::Manifest = serde_json::from_value(
        json!({ "artifact": "ctf/web:1", "port": 8000, "env_allow": [], "max_ttl_secs": 600 }),
    )
    .unwrap();
    let mut cfg = kymo_lab::server::LabConfig::new("plain", LAB_TOKEN);
    cfg.manifests = vec![manifest];
    let plain = kymo_lab::server::Lab::new(cfg, Arc::new(FakeRuntime::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(kymo_lab::server::serve(
        listener,
        kymo_lab::server::router(plain),
        None,
        std::future::pending(),
    ));
    let (_, added) = lab
        .admin(
            "POST",
            "/admin/labs",
            Some(json!({ "name": "Plain", "api_url": url, "token": LAB_TOKEN, "public_host": "203.0.113.9" })),
        )
        .await;
    let id = added["lab"]["id"].as_i64().unwrap();
    lab.admin(
        "PUT",
        &format!("/admin/events/{}/lab", lab.event_id),
        Some(json!({ "lab_host_id": id })),
    )
    .await;
    let (s, b) = lab.network("vpn").await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert!(
        b["error"].as_str().unwrap().contains("no VPN gateway"),
        "{b}"
    );
}

#[tokio::test]
async fn starting_stopping_and_profiles_are_audited_with_who_and_from_where() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box A").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    assert_eq!(lab.network("vpn").await.0, StatusCode::OK);
    lab.profile(&pat).await;
    lab.act(&pat, "POST", a, "").await;
    lab.act(&pat, "DELETE", a, "").await;

    let (s, page) = lab
        .admin("GET", "/admin/monitor/audit?per_page=100", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{page}");
    let entries = page["entries"].as_array().unwrap();
    let find = |action: &str| {
        entries
            .iter()
            .find(|e| e["action"] == action)
            .unwrap_or_else(|| panic!("no {action} in {page}"))
    };
    for action in ["vpn_profile", "instance_start", "instance_stop"] {
        let e = find(action);
        assert_eq!(e["ip"], "10.0.0.1", "{e}");
        assert_eq!(e["actor_name"], "pat");
        assert_eq!(e["actor_role"], "player");
    }
    let start = find("instance_start");
    assert!(
        start["detail"].as_str().unwrap().contains("Box A"),
        "{start}"
    );
    assert!(
        start["detail"]
            .as_str()
            .unwrap()
            .contains("address=10.77.0.10:8000"),
        "{start}"
    );
    // The filter offers what has actually been recorded.
    let actions: Vec<&str> = page["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a[0].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"instance_start") && actions.contains(&"vpn_profile"));

    // Search by address, by player, and page through.
    let (_, by_ip) = lab
        .admin("GET", "/admin/monitor/audit?q=10.0.0.1", None)
        .await;
    assert!(by_ip["total"].as_i64().unwrap() >= 3);
    let (_, by_name) = lab
        .admin(
            "GET",
            "/admin/monitor/audit?q=pat&action=instance_start",
            None,
        )
        .await;
    assert_eq!(by_name["total"], 1);
    let (_, none) = lab
        .admin("GET", "/admin/monitor/audit?q=203.0.113.99", None)
        .await;
    assert_eq!(none["total"], 0);
    let (_, wild) = lab.admin("GET", "/admin/monitor/audit?q=%25", None).await;
    assert_eq!(
        wild["total"], 0,
        "a percent sign is searched for, not a wildcard"
    );
    let (_, first) = lab
        .admin("GET", "/admin/monitor/audit?per_page=2&page=1", None)
        .await;
    let (_, second) = lab
        .admin("GET", "/admin/monitor/audit?per_page=2&page=2", None)
        .await;
    assert_eq!(first["entries"].as_array().unwrap().len(), 2);
    assert_ne!(first["entries"][0]["id"], second["entries"][0]["id"]);
}

#[tokio::test]
async fn a_machine_is_given_as_an_address_alone_and_a_service_as_address_and_port() {
    let lab = Lab::new(true, "solo").await;
    let a = lab.challenge("Box A").await;
    let (pat, _) = lab.player("pat").await;
    lab.register(&pat).await;
    lab.set_state("live").await;
    assert_eq!(lab.network("vpn").await.0, StatusCode::OK);
    let (_, service) = lab.act(&pat, "POST", a, "").await;
    assert_eq!(
        (service["machine"].clone(), service["port"].clone()),
        (json!(false), json!(8000))
    );

    // The lab host marks it a machine: every port is open on the address, so there is no port.
    sqlx::query("UPDATE instance SET machine = 1 WHERE state = 'running'")
        .execute(&lab.state.pool)
        .await
        .unwrap();
    let (_, machine) = lab.act(&pat, "GET", a, "").await;
    assert_eq!(machine["machine"], true);
    assert_eq!(machine["host"], "10.77.0.10");
    assert!(
        machine["port"].is_null() && machine["services"] == json!([]),
        "{machine}"
    );
    let (_, mine) = lab
        .call(
            "GET",
            &format!("/events/{}/instances", lab.slug),
            Some(&pat),
            None,
        )
        .await;
    assert_eq!(mine["instances"][0]["machine"], true);
    assert!(mine["instances"][0]["port"].is_null());
}
