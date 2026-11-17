use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kymo_server::{AppState, db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";
const PLAYER_PASSWORD: &str = "a long player password";
/// 2026-01-01T00:00:00Z
const T0: i64 = 1_767_225_600;

struct World {
    app: Router,
}

async fn world() -> (World, AppState) {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let clock = Arc::new(AtomicI64::new(T0));
    let shared = Arc::clone(&clock);
    let state = AppState::new(pool, PASSWORD)
        .unwrap()
        .with_clock(Arc::new(move || shared.load(Ordering::SeqCst)));
    (
        World {
            app: router(state.clone()),
        },
        state,
    )
}

struct Reply {
    status: StatusCode,
    bytes: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    content_type: Option<&str>,
    body: Vec<u8>,
    ip: Option<&str>,
) -> Reply {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    if let Some(ct) = content_type {
        req = req.header("content-type", ct);
    }
    if let Some(ip) = ip {
        let addr: SocketAddr = format!("{ip}:4000").parse().unwrap();
        req = req.extension(ConnectInfo(addr));
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Reply { status, bytes }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (ct, bytes) = match body {
        Some(v) => (Some("application/json"), v.to_string().into_bytes()),
        None => (None, Vec::new()),
    };
    let r = send(app, method, uri, token, ct, bytes, None).await;
    (r.status, r.json())
}

async fn admin_token(app: &Router) -> String {
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

/// Registers a player and returns (token, user id).
async fn player(app: &Router, name: &str) -> (String, i64) {
    let (status, body) = call(
        app,
        "POST",
        "/api/v1/auth/register",
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

const LAB_TOKEN: &str = "test-lab-token-with-enough-length-123456";

/// A lab host in this process whose runner answers from the submitted source: `SOLVE` adds the
/// two numbers, `PARTIAL` is right only for the sample, `CRASH` exits with an error.
async fn lab_host() -> String {
    let manifest: kymo_lab::Manifest = serde_json::from_value(json!({
        "artifact": "judge/py:1", "runtime": "docker",
        "judge": { "file": "main.py", "run": ["python3", "main.py"], "label": "Python 3" }
    }))
    .unwrap();
    let mut cfg = kymo_lab::server::LabConfig::new("judge-lab", LAB_TOKEN);
    cfg.manifests = vec![manifest];
    cfg.judge_dir = std::env::temp_dir().join(format!("kymo-code-test-{}", std::process::id()));
    let lab = kymo_lab::server::Lab::new(cfg, Arc::new(kymo_instances::FakeRuntime::new()));
    lab.set_judge_runner(Arc::new(kymo_instances::FakeJobRunner::new(|job| {
        let source = std::fs::read_to_string(job.dir.join("main.py")).unwrap_or_default();
        let input = String::from_utf8_lossy(&job.stdin).into_owned();
        let sum: i64 = input
            .split_whitespace()
            .filter_map(|n| n.parse::<i64>().ok())
            .sum();
        let ok = |out: String| kymo_instances::JobOutcome {
            exit_code: Some(0),
            stdout: out.into_bytes(),
            wall_ms: 700,
            ..Default::default()
        };
        if source.contains("CRASH") {
            kymo_instances::JobOutcome {
                exit_code: Some(1),
                stderr: b"Traceback".to_vec(),
                wall_ms: 700,
                ..Default::default()
            }
        } else if source.contains("SOLVE") {
            ok(format!("{sum}\n"))
        } else if source.contains("PARTIAL") && input.trim() == "1 2" {
            ok("3\n".into())
        } else {
            ok("0\n".into())
        }
    })));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(kymo_lab::server::serve(
        listener,
        kymo_lab::server::router(lab),
        None,
        std::future::pending(),
    ));
    url
}

struct Site {
    w: World,
    admin: String,
    event_id: i64,
    slug: String,
    category: i64,
}

impl Site {
    async fn new() -> Site {
        let (w, _) = world().await;
        let admin = admin_token(&w.app).await;
        let (_, event) = call(
            &w.app,
            "POST",
            "/api/v1/admin/events",
            Some(&admin),
            Some(json!({ "name": "Code Night", "kind": "jeopardy", "mode": "solo" })),
        )
        .await;
        let event_id = event["id"].as_i64().unwrap();
        let (_, category) = call(
            &w.app,
            "POST",
            &format!("/api/v1/admin/events/{event_id}/categories"),
            Some(&admin),
            Some(json!({ "name": "Coding" })),
        )
        .await;
        let site = Site {
            w,
            admin,
            event_id,
            slug: event["slug"].as_str().unwrap().to_owned(),
            category: category["id"].as_i64().unwrap(),
        };
        site.set_state("registration_open").await;
        site
    }

    async fn admin(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        call(
            &self.w.app,
            method,
            &format!("/api/v1{uri}"),
            Some(&self.admin),
            body,
        )
        .await
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

    async fn use_lab(&self) {
        let url = lab_host().await;
        let (s, added) = self.admin("POST", "/admin/labs", Some(json!({ "name": "Judge", "api_url": url, "token": LAB_TOKEN, "public_host": "203.0.113.9" }))).await;
        assert_eq!(s, StatusCode::CREATED, "{added}");
        let id = added["lab"]["id"].as_i64().unwrap();
        let (s, b) = self
            .admin(
                "PUT",
                &format!("/admin/events/{}/lab", self.event_id),
                Some(json!({ "lab_host_id": id })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }

    async fn coding_challenge(&self) -> i64 {
        let (s, made) = self
            .admin(
                "POST",
                &format!("/admin/events/{}/challenges", self.event_id),
                Some(json!({ "category_id": self.category, "title": "Add", "kind": "code", "scoring": { "kind": "static", "points": 100 } })),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{made}");
        let id = made["id"].as_i64().unwrap();
        let (s, b) = self
            .admin(
                "PUT",
                &format!("/admin/challenges/{id}/code"),
                Some(config()),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        self.admin("POST", &format!("/admin/challenges/{id}/publish"), None)
            .await;
        id
    }

    async fn run(&self, token: &str, challenge: i64, source: &str) -> (StatusCode, Value) {
        call(
            &self.w.app,
            "POST",
            &format!("/api/v1/events/{}/challenges/{challenge}/code", self.slug),
            Some(token),
            Some(json!({ "language": "python", "source": source })),
        )
        .await
    }
}

fn config() -> Value {
    json!({
        "languages": [ { "id": "python", "label": "Python 3", "artifact": "judge/py:1" } ],
        "cases": [
            { "input": "1 2", "expected": "3", "sample": true },
            { "input": "40 2", "expected": "42" }
        ],
        "time_limit_ms": 1000,
        "memory_mb": 64
    })
}

#[tokio::test]
async fn only_a_run_that_passes_every_test_solves_and_hidden_tests_stay_hidden() {
    let site = Site::new().await;
    site.use_lab().await;
    let id = site.coding_challenge().await;
    let (pat, _) = player(&site.w.app, "pat").await;
    call(
        &site.w.app,
        "POST",
        &format!("/api/v1/events/{}/register", site.slug),
        Some(&pat),
        None,
    )
    .await;
    site.set_state("live").await;

    // The player sees the languages and the sample, never the hidden test.
    let (s, list) = call(
        &site.w.app,
        "GET",
        &format!("/api/v1/events/{}/challenges", site.slug),
        Some(&pat),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let text = list.to_string();
    assert!(
        text.contains("\"1 2\"") && text.contains("Python 3"),
        "{text}"
    );
    assert!(
        !text.contains("\"40 2\"") && !text.contains("\"42\""),
        "a hidden test leaked: {text}"
    );

    // A run that passes only the sample is not a solve, and shows nothing of the hidden test.
    let (s, partial) = site.run(&pat, id, "# PARTIAL").await;
    assert_eq!(s, StatusCode::OK, "{partial}");
    assert_eq!(
        (
            partial["verdict"].as_str(),
            partial["passed"].as_i64(),
            partial["total"].as_i64()
        ),
        (Some("incorrect"), Some(1), Some(2))
    );
    assert_eq!(partial["cases"][0]["verdict"], "accepted");
    assert_eq!(partial["cases"][1]["verdict"], "wrong_answer");
    assert!(
        partial["cases"][0].get("expected").is_some(),
        "the sample is shown"
    );
    assert!(
        partial["cases"][1].get("expected").is_none()
            && partial["cases"][1].get("stdout").is_none(),
        "{partial}"
    );

    // A crash is a runtime error, not a pass.
    let (_, crash) = site.run(&pat, id, "# CRASH").await;
    assert_eq!(crash["cases"][0]["verdict"], "runtime_error");

    // The flag endpoint is no way in: the placeholder flag must not solve it.
    let (s, b) = call(
        &site.w.app,
        "POST",
        &format!("/api/v1/events/{}/challenges/{id}/submit", site.slug),
        Some(&pat),
        Some(json!({ "flag": "code" })),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");

    // A run that passes everything solves it for the points, once.
    let (s, win) = site.run(&pat, id, "# SOLVE").await;
    assert_eq!(s, StatusCode::OK, "{win}");
    assert_eq!(
        (
            win["verdict"].as_str(),
            win["correct"].as_bool(),
            win["points"].as_i64()
        ),
        (Some("correct"), Some(true), Some(100))
    );
    let (s, _) = site.run(&pat, id, "# SOLVE").await;
    assert_eq!(s, StatusCode::CONFLICT);

    // The organizer sees the hidden tests and can try a reference solution, with nothing recorded.
    let (_, admin_list) = site
        .admin(
            "GET",
            &format!("/admin/events/{}/challenges", site.event_id),
            None,
        )
        .await;
    assert!(admin_list.to_string().contains("\"40 2\""));
    let (s, tried) = site
        .admin(
            "POST",
            &format!("/admin/challenges/{id}/code/test"),
            Some(json!({ "language": "python", "source": "# SOLVE" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{tried}");
    assert_eq!(tried["cases"][1]["expected"], "42");
}

#[tokio::test]
async fn a_coding_challenge_is_checked_when_it_is_set_up_and_needs_a_lab_host_to_run() {
    let site = Site::new().await;
    let id = site.coding_challenge().await;

    // Bad configs and the wrong kind of challenge are refused.
    let mut bad = config();
    bad["cases"] = json!([]);
    let (s, _) = site
        .admin("PUT", &format!("/admin/challenges/{id}/code"), Some(bad))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, plain) = site
        .admin(
            "POST",
            &format!("/admin/events/{}/challenges", site.event_id),
            Some(json!({ "category_id": site.category, "title": "Flag", "scoring": { "kind": "static", "points": 10 }, "flag": { "kind": "static", "value": "KYMO{x}" } })),
        )
        .await;
    let (s, _) = site
        .admin(
            "PUT",
            &format!("/admin/challenges/{}/code", plain["id"]),
            Some(config()),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // With no lab host on the event, a run says why it cannot run, and costs nothing.
    let (pat, _) = player(&site.w.app, "pat").await;
    call(
        &site.w.app,
        "POST",
        &format!("/api/v1/events/{}/register", site.slug),
        Some(&pat),
        None,
    )
    .await;
    site.set_state("live").await;
    let (s, b) = site.run(&pat, id, "# SOLVE").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{b}");
    assert!(b["error"].as_str().unwrap().contains("lab host"), "{b}");

    // Source that is empty or too long never reaches a lab host.
    let (s, _) = site.run(&pat, id, "   ").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = site.run(&pat, id, &"x".repeat(70_000)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        &site.w.app,
        "POST",
        &format!("/api/v1/events/{}/challenges/{id}/code", site.slug),
        Some(&pat),
        Some(json!({ "language": "cobol", "source": "x" })),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_coding_challenge_survives_an_export_and_import_and_the_placeholder_flag_still_solves_nothing()
 {
    let site = Site::new().await;
    let id = site.coding_challenge().await;
    let (s, bundle) = site
        .admin(
            "GET",
            &format!("/admin/events/{}/export", site.event_id),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{bundle}");
    let part = &bundle["challenges"][0];
    assert_eq!(part["kind"], "code", "{part}");
    assert_eq!(part["code"]["cases"][1]["expected"], "42");

    let (s, imported) = site
        .admin("POST", "/admin/events/import", Some(bundle.clone()))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{imported}");
    let new_event = imported["id"].as_i64().unwrap();
    let (_, list) = site
        .admin(
            "GET",
            &format!("/admin/events/{new_event}/challenges"),
            None,
        )
        .await;
    assert_eq!(list[0]["kind"], "code", "{list}");
    assert_eq!(list[0]["code"]["time_limit_ms"], 1000);
    assert_ne!(list[0]["id"].as_i64().unwrap(), id);

    // A bundle that claims a coding challenge but carries no tests is refused, not turned into a
    // plain flag challenge whose placeholder flag anybody could submit.
    let mut broken = bundle;
    broken["challenges"][0]
        .as_object_mut()
        .unwrap()
        .remove("code");
    let (s, b) = site
        .admin("POST", "/admin/events/import", Some(broken))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
}

#[tokio::test]
async fn a_coding_challenge_cannot_be_published_before_it_has_tests() {
    let site = Site::new().await;
    let (_, made) = site
        .admin(
            "POST",
            &format!("/admin/events/{}/challenges", site.event_id),
            Some(json!({ "category_id": site.category, "title": "Empty", "kind": "code", "scoring": { "kind": "static", "points": 50 } })),
        )
        .await;
    let id = made["id"].as_i64().unwrap();
    assert_eq!(made["kind"], "code");
    let (s, b) = site
        .admin("POST", &format!("/admin/challenges/{id}/publish"), None)
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
}

async fn listed(site: &Site) -> Value {
    site.admin(
        "GET",
        &format!("/admin/events/{}/code-matches", site.event_id),
        None,
    )
    .await
    .1
}

const ORIGINAL: &str = "# SOLVE
def total(values):
    acc = 0
    for item in values:
        if item % 2 == 0:
            acc += item
        else:
            acc += item
    return acc

nums = [int(x) for x in input().split()]
print(total(nums))
";

/// The same program with other names, other comments and other spacing.
const DISGUISED: &str = "# my own work, SOLVE
def add_all(numbers):    # adds them
    result = 0
    for number in numbers:
        if number % 2 == 0:
            result += number
        else:
            result += number
    return result
values = [int(v) for v in input().split()]
print(add_all(values))
";

const DIFFERENT: &str = "# SOLVE
import sys
parts = sys.stdin.readline().split()
answer = 0
index = 0
while index < len(parts):
    answer = answer + int(parts[index])
    index = index + 1
sys.stdout.write(str(answer) + chr(10))
";

#[tokio::test]
async fn a_copied_solution_is_reported_to_the_organizers_and_an_honest_one_is_not() {
    let site = Site::new().await;
    site.use_lab().await;
    let id = site.coding_challenge().await;
    let mut tokens = Vec::new();
    for name in ["ann", "ben", "cyd"] {
        let (t, _) = player(&site.w.app, name).await;
        call(
            &site.w.app,
            "POST",
            &format!("/api/v1/events/{}/register", site.slug),
            Some(&t),
            None,
        )
        .await;
        tokens.push(t);
    }
    site.set_state("live").await;

    let (_, a) = site.run(&tokens[0], id, ORIGINAL).await;
    assert_eq!(a["verdict"], "correct", "{a}");
    assert!(
        listed(&site).await["matches"]
            .as_array()
            .unwrap()
            .is_empty(),
        "one solution has nothing to match"
    );

    // Ben hands in the same program with the names changed: reported, with who and how alike.
    let (_, b) = site.run(&tokens[1], id, DISGUISED).await;
    assert_eq!(b["verdict"], "correct", "{b}");
    let found = listed(&site).await;
    let matches = found["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1, "{found}");
    assert!(matches[0]["score"].as_i64().unwrap() >= 85, "{found}");
    assert_eq!(
        (
            matches[0]["name"].as_str(),
            matches[0]["other_name"].as_str(),
            matches[0]["challenge"].as_str()
        ),
        (Some("ben"), Some("ann"), Some("Add"))
    );
    assert!(
        site.admin("GET", "/admin/monitor/audit?action=code_similarity", None)
            .await
            .1["total"]
            .as_i64()
            .unwrap()
            >= 1
    );

    // A genuinely different program is not reported, and a player may not read any of it.
    let (_, c) = site.run(&tokens[2], id, DIFFERENT).await;
    assert_eq!(c["verdict"], "correct", "{c}");
    assert_eq!(listed(&site).await["matches"].as_array().unwrap().len(), 1);
    let (s, _) = call(
        &site.w.app,
        "GET",
        &format!("/api/v1/admin/events/{}/code-matches", site.event_id),
        Some(&tokens[2]),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Both sources can be read side by side, and a false alarm can be dismissed.
    let match_id = matches[0]["id"].as_i64().unwrap();
    let (s, detail) = site
        .admin("GET", &format!("/admin/code-matches/{match_id}"), None)
        .await;
    assert_eq!(s, StatusCode::OK, "{detail}");
    assert!(
        detail["a"]["source"].as_str().unwrap().contains("add_all")
            && detail["b"]["source"]
                .as_str()
                .unwrap()
                .contains("def total"),
        "{detail}"
    );
    assert_eq!(
        site.admin(
            "POST",
            &format!("/admin/code-matches/{match_id}/dismiss"),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert!(
        listed(&site).await["matches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        site.admin(
            "POST",
            &format!("/admin/code-matches/{match_id}/dismiss"),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
