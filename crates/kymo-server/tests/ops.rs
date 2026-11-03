use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::ops::{self, BackupConfig};
use kymo_server::{AppState, db, restore, router};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;
const DAY: i64 = 86_400;

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    bytes: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
}

struct World {
    app: Router,
    state: AppState,
    pool: SqlitePool,
    clock: Arc<AtomicI64>,
    /// Snapshots need a real database file, so every world gets its own.
    _dir: tempfile::TempDir,
}

impl World {
    async fn new(backups: Option<BackupConfig>) -> World {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("kymo.db").display());
        let pool = db::connect(&url).await.unwrap();
        let clock = Arc::new(AtomicI64::new(T0));
        let shared = Arc::clone(&clock);
        let mut state = AppState::new(pool.clone(), PASSWORD)
            .unwrap()
            .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
        if let Some(config) = backups {
            state = state.with_backups(config);
        }
        World {
            app: router(state.clone()),
            state,
            pool,
            clock,
            _dir: dir,
        }
    }

    fn set_time(&self, t: i64) {
        self.clock.store(t, Ordering::SeqCst);
    }

    async fn raw(&self, method: &str, uri: &str, token: Option<&str>, body: Vec<u8>) -> Reply {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"));
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        if !body.is_empty() {
            req = req.header("content-type", "application/json");
        }
        let addr: SocketAddr = "10.0.0.1:4000".parse().unwrap();
        let res = self
            .app
            .clone()
            .oneshot(
                req.extension(ConnectInfo(addr))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        Reply {
            status,
            headers,
            bytes,
        }
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let r = self
            .raw(
                method,
                uri,
                token,
                body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
            )
            .await;
        (r.status, r.json())
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
        assert_eq!(s, StatusCode::OK);
        b["token"].as_str().unwrap().to_owned()
    }

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

    async fn staff(&self, admin: &str, name: &str, role: &str) -> String {
        let (token, id) = self.player(name).await;
        let (s, b) = self
            .call(
                "PUT",
                &format!("/admin/users/{id}"),
                Some(admin),
                Some(json!({ "role": role })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        token
    }
}

fn private_backups(dir: &Path, keep: usize) -> BackupConfig {
    BackupConfig {
        dir: dir.join("backups"),
        every_secs: DAY,
        keep,
    }
}

// ---------- health ----------

#[tokio::test]
async fn liveness_is_public_and_not_logged_while_details_are_for_staff() {
    let w = World::new(None).await;
    let admin = w.admin().await;
    let r = w.raw("GET", "/health", None, vec![]).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json()["status"], "ok");
    assert!(
        r.json().get("database").is_none(),
        "no details for the public"
    );

    let (player, _) = w.player("pat").await;
    let monitor = w.staff(&admin, "mona", "monitor").await;
    assert_eq!(
        w.call("GET", "/admin/ops/health", None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        w.call("GET", "/admin/ops/health", Some(&player), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let (status, health) = w
        .call("GET", "/admin/ops/health", Some(&monitor), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{health}");
    assert_eq!(health["status"], "ok");
    assert_eq!(health["database"]["ok"], true);
    assert!(health["database"]["size_bytes"].as_i64().unwrap() > 0);
    assert_eq!(health["instances"]["enabled"], false);
    assert_eq!(health["backups"]["configured"], false);
    assert!(health["recent_errors"].as_array().unwrap().is_empty());
    assert!(health["traffic"]["requests"].as_i64().unwrap() >= 2);

    let (_, log) = w
        .call(
            "GET",
            "/admin/monitor/requests?route=/api/v1/health",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(log["total"], 0, "probes are not logged");
}

#[tokio::test]
async fn health_lists_each_open_event_with_its_activity() {
    let w = World::new(None).await;
    let admin = w.admin().await;
    let (s, event) = w
        .call(
            "POST",
            "/admin/events",
            Some(&admin),
            Some(json!({ "name": "Spring", "kind": "jeopardy", "mode": "solo" })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (_, health) = w.call("GET", "/admin/ops/health", Some(&admin), None).await;
    let listed = health["events"].as_array().unwrap();
    assert!(
        listed
            .iter()
            .any(|e| e["slug"] == event["slug"] && e["state"] == "draft" && e["participants"] == 0)
    );
    w.call(
        "POST",
        &format!("/admin/events/{}/state", event["id"]),
        Some(&admin),
        Some(json!({ "state": "live" })),
    )
    .await;
    w.call(
        "POST",
        &format!("/admin/events/{}/state", event["id"]),
        Some(&admin),
        Some(json!({ "state": "ended" })),
    )
    .await;
    w.call(
        "POST",
        &format!("/admin/events/{}/state", event["id"]),
        Some(&admin),
        Some(json!({ "state": "archived" })),
    )
    .await;
    let (_, health) = w.call("GET", "/admin/ops/health", Some(&admin), None).await;
    assert!(
        !health["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["slug"] == event["slug"]),
        "archived events are left out"
    );
}

// ---------- backups ----------

#[tokio::test]
async fn the_downloaded_backup_is_a_valid_snapshot_and_only_the_organizer_gets_it() {
    let dir = tempfile::tempdir().unwrap();
    let w = World::new(None).await;
    let admin = w.admin().await;
    let monitor = w.staff(&admin, "mona", "monitor").await;
    let (player, _) = w.player("pat").await;
    assert_eq!(
        w.call("GET", "/admin/ops/backup/download", None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        w.call("GET", "/admin/ops/backup/download", Some(&player), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.call("GET", "/admin/ops/backup/download", Some(&monitor), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let r = w
        .raw("GET", "/admin/ops/backup/download", Some(&admin), vec![])
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers["cache-control"], "no-store");
    assert!(
        r.headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"kymo-backup-")
    );
    assert!(r.bytes.starts_with(b"SQLite format 3"));

    let file = dir.path().join("download.db");
    std::fs::write(&file, &r.bytes).unwrap();
    restore::verify(&file).await.unwrap();
    let copy = db::connect(&format!("sqlite://{}", file.display()))
        .await
        .unwrap();
    let users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user WHERE username IN ('pat', 'mona')")
            .fetch_one(&copy)
            .await
            .unwrap();
    assert_eq!(users, 2, "the snapshot holds the data");

    let (_, audit) = w
        .call(
            "GET",
            "/admin/monitor/audit?action=backup_download",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(audit["total"], 1);
}

#[tokio::test]
async fn scheduled_backups_are_private_rotated_and_only_written_when_due() {
    let dir = tempfile::tempdir().unwrap();
    let w = World::new(Some(private_backups(dir.path(), 2))).await;
    let admin = w.admin().await;

    ops::maintain(&w.state).await.unwrap();
    let (_, listing) = w
        .call("GET", "/admin/ops/backups", Some(&admin), None)
        .await;
    assert_eq!(
        listing["backups"].as_array().unwrap().len(),
        1,
        "first run writes one"
    );

    w.set_time(T0 + DAY - 1);
    ops::maintain(&w.state).await.unwrap();
    let (_, listing) = w
        .call("GET", "/admin/ops/backups", Some(&admin), None)
        .await;
    assert_eq!(
        listing["backups"].as_array().unwrap().len(),
        1,
        "not due yet"
    );

    for day in 1..=3 {
        w.set_time(T0 + day * DAY);
        ops::maintain(&w.state).await.unwrap();
    }
    let (_, listing) = w
        .call("GET", "/admin/ops/backups", Some(&admin), None)
        .await;
    let names: Vec<&str> = listing["backups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "keep=2: {names:?}");
    assert!(names[0] > names[1], "newest first");
    assert!(names[0].contains(&format!("{:012}", T0 + 3 * DAY)));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.path().join("backups")), 0o700);
        assert_eq!(mode(&dir.path().join("backups").join(names[0])), 0o600);
    }
    restore::verify(&dir.path().join("backups").join(names[0]))
        .await
        .unwrap();

    let (_, health) = w.call("GET", "/admin/ops/health", Some(&admin), None).await;
    assert_eq!(health["jobs"]["backup"]["error"], Value::Null);
    assert_eq!(health["backups"]["configured"], true);

    w.set_time(T0 + 3 * DAY + 5);
    let (status, made) = w
        .call("POST", "/admin/ops/backups", Some(&admin), None)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
}

#[tokio::test]
async fn backups_need_configuration_and_a_failing_directory_degrades_health() {
    let w = World::new(None).await;
    let admin = w.admin().await;
    let (status, body) = w
        .call("POST", "/admin/ops/backups", Some(&admin), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (_, listing) = w
        .call("GET", "/admin/ops/backups", Some(&admin), None)
        .await;
    assert_eq!(listing["configured"], false);

    // A path under a regular file cannot become a directory.
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, b"x").unwrap();
    let broken = World::new(Some(BackupConfig {
        dir: blocker.join("sub"),
        every_secs: DAY,
        keep: 2,
    }))
    .await;
    let admin = broken.admin().await;
    let (status, _) = broken
        .call("POST", "/admin/ops/backups", Some(&admin), None)
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let (_, health) = broken
        .call("GET", "/admin/ops/health", Some(&admin), None)
        .await;
    assert_eq!(health["status"], "degraded");
    assert!(health["jobs"]["backup"]["error"].is_string());
}

// ---------- restore ----------

async fn file_db(path: &Path) -> SqlitePool {
    db::connect(&format!("sqlite://{}", path.display()))
        .await
        .unwrap()
}

#[tokio::test]
async fn restore_swaps_in_a_verified_backup_and_keeps_the_old_database() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    let backup = dir.path().join("backup.db");

    let pool = file_db(&live).await;
    sqlx::query("INSERT INTO event (kind, mode, name, slug, created_at) VALUES ('jeopardy', 'solo', 'Before', 'before', 1)")
        .execute(&pool)
        .await
        .unwrap();
    ops::snapshot_to(&pool, &backup).await.unwrap();
    sqlx::query("DELETE FROM event")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let kept = restore::restore(&live, &backup, false, 1234)
        .await
        .unwrap()
        .expect("kept");
    assert!(kept.to_string_lossy().ends_with("live.db.pre-restore-1234"));

    let after = file_db(&live).await;
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM event")
        .fetch_all(&after)
        .await
        .unwrap();
    assert_eq!(names, vec!["Before"]);
    let old = file_db(&kept).await;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event")
        .fetch_one(&old)
        .await
        .unwrap();
    assert_eq!(n, 0, "the post-backup state was preserved");
}

#[tokio::test]
async fn restore_refuses_damaged_foreign_and_newer_backups_and_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    let pool = file_db(&live).await;
    pool.close().await;
    let before = std::fs::read(&live).unwrap();

    let garbage = dir.path().join("garbage.db");
    std::fs::write(
        &garbage,
        b"this is not sqlite at all, just text padding.....",
    )
    .unwrap();
    assert!(restore::restore(&live, &garbage, false, 1).await.is_err());

    let missing = dir.path().join("missing.db");
    assert!(
        restore::restore(&live, &missing, false, 1)
            .await
            .unwrap_err()
            .contains("not a file")
    );

    let foreign = dir.path().join("foreign.db");
    let other = db_plain(&foreign).await;
    sqlx::query("CREATE TABLE notes (id INTEGER)")
        .execute(&other)
        .await
        .unwrap();
    other.close().await;
    assert!(
        restore::restore(&live, &foreign, false, 1)
            .await
            .unwrap_err()
            .contains("not a Kymo database")
    );

    let newer = dir.path().join("newer.db");
    let src = file_db(&dir.path().join("src.db")).await;
    ops::snapshot_to(&src, &newer).await.unwrap();
    let edit = db_plain(&newer).await;
    sqlx::query("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (999999, 'future', 1, x'00', 0)")
        .execute(&edit)
        .await
        .unwrap();
    edit.close().await;
    assert!(
        restore::restore(&live, &newer, false, 1)
            .await
            .unwrap_err()
            .contains("newer Kymo")
    );

    assert_eq!(
        std::fs::read(&live).unwrap(),
        before,
        "a refused restore changes nothing"
    );
    assert!(!dir.path().join("live.db.restoring").exists());
}

async fn db_plain(path: &Path) -> SqlitePool {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
        .unwrap()
}

#[test]
fn database_paths_come_from_sqlite_urls() {
    assert_eq!(
        restore::path_of_url("sqlite://kymo.db").unwrap(),
        Path::new("kymo.db")
    );
    assert_eq!(
        restore::path_of_url("sqlite:data/k.db?mode=rwc").unwrap(),
        Path::new("data/k.db")
    );
    assert!(restore::path_of_url("sqlite::memory:").is_err());
    assert!(restore::path_of_url("postgres://x").is_err());
}

// ---------- housekeeping ----------

#[tokio::test]
async fn cleanup_removes_expired_sessions_and_old_request_rows() {
    let w = World::new(None).await;
    let admin = w.admin().await;
    sqlx::query("INSERT INTO token (hash, kind, label, created_at, expires_at) VALUES ('dead', 'session', '', 1, ?)")
        .bind(T0 - 1)
        .execute(&w.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO token (hash, kind, label, created_at, expires_at) VALUES ('alive', 'session', '', 1, ?)")
        .bind(T0 + 30 * DAY)
        .execute(&w.pool)
        .await
        .unwrap();
    w.call("GET", "/ladder", None, None).await;

    w.set_time(T0 + 8 * DAY);
    let (status, report) = w
        .call("POST", "/admin/ops/cleanup", Some(&admin), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["expired_sessions"], 1);
    assert!(report["log_rows"].as_i64().unwrap() >= 1);
    let alive: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM token WHERE hash = 'alive'")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(alive, 1, "a session that has not expired is kept");
    let (player, _) = w.player("pat").await;
    assert_eq!(
        w.call("POST", "/admin/ops/cleanup", Some(&player), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

// ---------- event bundles ----------

struct Seeded {
    admin: String,
    event_id: i64,
}

async fn seeded_event(w: &World) -> Seeded {
    let admin = w.admin().await;
    let (_, event) = w
        .call(
            "POST",
            "/admin/events",
            Some(&admin),
            Some(json!({
                "name": "Spring CTF", "kind": "jeopardy", "mode": "team",
                "description": "rules here", "starts_at": T0 + DAY, "ends_at": T0 + 2 * DAY,
            })),
        )
        .await;
    let id = event["id"].as_i64().unwrap();
    let (_, web) = w
        .call(
            "POST",
            &format!("/admin/events/{id}/categories"),
            Some(&admin),
            Some(json!({ "name": "Web" })),
        )
        .await;
    w.call(
        "POST",
        &format!("/admin/events/{id}/categories"),
        Some(&admin),
        Some(json!({ "name": "Pwn" })),
    )
    .await;
    w.call("PUT", &format!("/admin/events/{id}/jeopardy"), Some(&admin), Some(json!({
        "tie_breaks": ["most_solves"], "rate_max_attempts": 7, "rate_window_secs": 30, "rate_lockout_secs": 5,
    })))
    .await;
    let mut first = 0;
    for (title, extra) in [
        (
            "Warmup",
            json!({ "scoring": { "kind": "static", "points": 100 } }),
        ),
        (
            "Box",
            json!({
                "scoring": { "kind": "dynamic", "initial": 500, "minimum": 100, "decay": 20, "curve": "parabolic" },
                "requires_instance": true,
                "instance_config": { "image": "ctf/web:1", "port": 80, "cpu_millis": 200, "memory_mb": 128, "pids": 64, "ttl_secs": 600 },
            }),
        ),
    ] {
        let mut body = json!({
            "category_id": web["id"], "title": title, "description": format!("{title} text"),
            "scoring": { "kind": "static", "points": 100 },
            "flag": { "kind": "static", "value": format!("KYMO{{{title}}}") },
            "flag_prefix": "KYMO{", "flag_suffix": "}", "max_attempts": 9,
        });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (s, c) = w
            .call(
                "POST",
                &format!("/admin/events/{id}/challenges"),
                Some(&admin),
                Some(body),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{c}");
        let cid = c["id"].as_i64().unwrap();
        if first == 0 {
            first = cid;
            w.call(
                "POST",
                &format!("/admin/challenges/{cid}/hints"),
                Some(&admin),
                Some(json!({ "content": "look closer", "cost": 25 })),
            )
            .await;
            let (s, a) = w
                .call("POST", &format!("/admin/challenges/{cid}/attachments"), Some(&admin), Some(json!({
                    "name": "notes.txt", "mime": "text/plain", "data_base64": "aGVsbG8gd29ybGQ=",
                })))
                .await;
            assert_eq!(s, StatusCode::CREATED, "{a}");
            w.call(
                "POST",
                &format!("/admin/challenges/{cid}/publish"),
                Some(&admin),
                None,
            )
            .await;
        }
    }
    Seeded {
        admin,
        event_id: id,
    }
}

#[tokio::test]
async fn a_bundle_round_trips_into_a_new_draft_event_with_everything_intact() {
    let w = World::new(None).await;
    let s = seeded_event(&w).await;
    let r = w
        .raw(
            "GET",
            &format!("/admin/events/{}/export", s.event_id),
            Some(&s.admin),
            vec![],
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers["cache-control"], "no-store");
    assert!(
        r.headers["content-disposition"]
            .to_str()
            .unwrap()
            .contains("spring-ctf.kymo-event.json")
    );
    let bundle = r.json();
    assert_eq!(bundle["format"], "kymo-event");
    assert_eq!(bundle["challenges"].as_array().unwrap().len(), 2);
    assert_eq!(
        bundle["challenges"][0]["flag"]["value"], "KYMO{Warmup}",
        "flags are part of the bundle"
    );

    let (status, imported) = {
        let r = w
            .raw(
                "POST",
                "/admin/events/import",
                Some(&s.admin),
                r.bytes.clone(),
            )
            .await;
        (r.status, r.json())
    };
    assert_eq!(status, StatusCode::CREATED, "{imported}");
    assert_eq!(imported["slug"], "spring-ctf-2");
    assert_eq!(imported["state"], "draft");
    assert_eq!(
        imported["starts_at"],
        Value::Null,
        "the schedule is cleared by default"
    );
    let new_id = imported["id"].as_i64().unwrap();

    let (_, list) = w
        .call(
            "GET",
            &format!("/admin/events/{new_id}/challenges"),
            Some(&s.admin),
            None,
        )
        .await;
    let challenges = list.as_array().unwrap();
    assert_eq!(challenges.len(), 2);
    assert!(
        challenges.iter().all(|c| c["state"] == "draft"),
        "imported as drafts"
    );
    let warm = challenges.iter().find(|c| c["title"] == "Warmup").unwrap();
    let (_, shown) = w
        .call(
            "GET",
            &format!("/admin/challenges/{}", warm["id"]),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(shown["flag"]["value"], "KYMO{Warmup}");
    assert_eq!(shown["max_attempts"], 9);
    assert_eq!(shown["hints"][0]["content"], "look closer");
    assert_eq!(shown["hints"][0]["cost"], 25);
    assert_eq!(shown["attachments"][0]["name"], "notes.txt");
    let box_ch = challenges.iter().find(|c| c["title"] == "Box").unwrap();
    let (_, shown) = w
        .call(
            "GET",
            &format!("/admin/challenges/{}", box_ch["id"]),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(shown["scoring"]["curve"], "parabolic");
    assert_eq!(shown["instance_config"]["image"], "ctf/web:1");

    let (_, settings) = w
        .call(
            "GET",
            &format!("/admin/events/{new_id}/jeopardy"),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(settings["rate_max_attempts"], 7);
    assert_eq!(settings["tie_breaks"][0], "most_solves");
    let (_, cats) = w
        .call(
            "GET",
            &format!("/admin/events/{new_id}/categories"),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(cats.as_array().unwrap().len(), 2);

    let (_, audit) = w
        .call(
            "GET",
            "/admin/monitor/audit?per_page=100",
            Some(&s.admin),
            None,
        )
        .await;
    let actions: Vec<&str> = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(
        actions.contains(&"event_export") && actions.contains(&"event_import"),
        "{actions:?}"
    );
}

#[tokio::test]
async fn restoring_options_keep_the_schedule_and_the_published_state() {
    let w = World::new(None).await;
    let s = seeded_event(&w).await;
    let bytes = w
        .raw(
            "GET",
            &format!("/admin/events/{}/export", s.event_id),
            Some(&s.admin),
            vec![],
        )
        .await
        .bytes;
    let r = w
        .raw(
            "POST",
            "/admin/events/import?keep_times=true&keep_state=true",
            Some(&s.admin),
            bytes,
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json());
    let imported = r.json();
    assert_eq!(imported["starts_at"], T0 + DAY);
    let (_, list) = w
        .call(
            "GET",
            &format!("/admin/events/{}/challenges", imported["id"]),
            Some(&s.admin),
            None,
        )
        .await;
    let states: Vec<(&str, &str)> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["title"].as_str().unwrap(), c["state"].as_str().unwrap()))
        .collect();
    assert!(
        states.contains(&("Warmup", "published")) && states.contains(&("Box", "draft")),
        "{states:?}"
    );
}

#[tokio::test]
async fn a_bad_bundle_is_rejected_whole_and_leaves_no_event_behind() {
    let w = World::new(None).await;
    let s = seeded_event(&w).await;
    let good: Value = w
        .raw(
            "GET",
            &format!("/admin/events/{}/export", s.event_id),
            Some(&s.admin),
            vec![],
        )
        .await
        .json();
    let before = event_count(&w, &s.admin).await;

    let mut tampered = good.clone();
    tampered["challenges"][0]["attachments"][0]["data_base64"] = json!("ZXZpbA==");
    let mut unknown = good.clone();
    unknown["challenges"][0]["category"] = json!("Crypto");
    let mut flag = good.clone();
    flag["challenges"][1]["flag"] = json!({ "kind": "regex", "pattern": "(unclosed" });
    let mut version = good.clone();
    version["version"] = json!(2);
    let mut instance = good.clone();
    instance["challenges"][1]["instance_config"]["image"] = json!("-v /:/host");
    let mut dup = good.clone();
    dup["categories"][1]["name"] = json!("Web");

    for (label, body) in [
        ("checksum", tampered),
        ("category", unknown),
        ("regex", flag),
        ("version", version),
        ("instance", instance),
        ("duplicate", dup),
    ] {
        let r = w
            .raw(
                "POST",
                "/admin/events/import",
                Some(&s.admin),
                body.to_string().into_bytes(),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{label}: {}", r.json());
    }
    let r = w
        .raw(
            "POST",
            "/admin/events/import",
            Some(&s.admin),
            b"{not json".to_vec(),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        event_count(&w, &s.admin).await,
        before,
        "no half imported events"
    );
}

#[tokio::test]
async fn bundles_are_for_challenge_managers_and_jeopardy_events_only() {
    let w = World::new(None).await;
    let s = seeded_event(&w).await;
    let (player, _) = w.player("pat").await;
    let uri = format!("/admin/events/{}/export", s.event_id);
    assert_eq!(
        w.call("GET", &uri, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        w.call("GET", &uri, Some(&player), None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.raw(
            "POST",
            "/admin/events/import",
            Some(&player),
            b"{}".to_vec()
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    let (_, t) = w
        .call(
            "POST",
            "/admin/events",
            Some(&s.admin),
            Some(json!({ "name": "Bracket", "kind": "tournament", "mode": "team" })),
        )
        .await;
    let (status, _) = w
        .call(
            "GET",
            &format!("/admin/events/{}/export", t["id"]),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

async fn event_count(w: &World, admin: &str) -> usize {
    let (_, list) = w.call("GET", "/admin/events", Some(admin), None).await;
    list.as_array().map_or(0, Vec::len)
}
