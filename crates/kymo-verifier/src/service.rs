//! The web service players open. Authentication is the network: the Verifier listens on an
//! address inside the player's own lab network, and the VPN gateway lets a tunnel reach only its
//! own lab. So there is no login. The page never shows more than check titles and states.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::definition::Loaded;
use crate::runner::{Context, Outcome, run_all};
use crate::session::{Refused, Session};

const LOG_LIMIT: usize = 500;

/// What the lab host tells a Verifier about its run, written to a private file at start.
#[derive(Debug, Clone, Deserialize)]
pub struct RunInfo {
    pub target_ip: String,
    #[serde(default)]
    pub target_ports: Vec<u16>,
    /// The flag to release when every check passes. Absent when the challenge has none to give.
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// Lets the lab host read the run log. Players do not know it.
    pub admin_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub at: i64,
    pub millis: u128,
    pub checks: Vec<LogCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogCheck {
    pub id: String,
    pub state: crate::runner::State,
    pub detail: String,
}

pub struct AppState {
    loaded: Loaded,
    context: Context,
    session: Mutex<Session>,
    log: Mutex<Vec<LogEntry>>,
    admin_token: String,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

pub fn system_clock() -> Arc<dyn Fn() -> i64 + Send + Sync> {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
    })
}

impl AppState {
    pub fn new(
        loaded: Loaded,
        context: Context,
        run: &RunInfo,
        clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Arc<AppState> {
        let session = Session::new(
            Arc::new(loaded.definition.clone()),
            run.flag.clone(),
            run.expires_at,
        );
        Arc::new(AppState {
            loaded,
            context,
            session: Mutex::new(session),
            log: Mutex::new(Vec::new()),
            admin_token: run.admin_token.clone(),
            clock,
        })
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/api/state", get(show))
        .route("/api/verify", post(verify))
        .route("/healthz", get(|| async { "ok" }))
        .route("/_admin/log", get(admin_log))
        .layer(axum::middleware::map_response(harden))
        .with_state(state)
}

/// Every answer is private to the player and carries no active content from outside.
async fn harden(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'none'",
        ),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn page() -> Html<&'static str> {
    Html(PAGE)
}

async fn show(State(app): State<Arc<AppState>>) -> Json<crate::session::View> {
    Json(lock(&app.session).view(app.now()))
}

async fn verify(State(app): State<Arc<AppState>>) -> Response {
    {
        let mut session = lock(&app.session);
        match session.begin(app.now()) {
            Ok(()) => {}
            Err(Refused::Busy) => {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "a run is already going" })),
                )
                    .into_response();
            }
            Err(Refused::Wait(secs)) => {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({ "error": "wait a little before the next run", "wait_secs": secs })),
                )
                    .into_response();
                if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
                    response.headers_mut().insert(header::RETRY_AFTER, v);
                }
                return response;
            }
        }
    }
    let started = std::time::Instant::now();
    let outcomes: Vec<Outcome> = run_all(&app.loaded, &app.context).await;
    let now = app.now();
    {
        let mut log = lock(&app.log);
        log.push(LogEntry {
            at: now,
            millis: started.elapsed().as_millis(),
            checks: app
                .loaded
                .definition
                .checks
                .iter()
                .zip(&outcomes)
                .map(|(check, o)| LogCheck {
                    id: check.id.clone(),
                    state: o.state,
                    detail: o.detail.clone(),
                })
                .collect(),
        });
        let excess = log.len().saturating_sub(LOG_LIMIT);
        log.drain(..excess);
    }
    let mut session = lock(&app.session);
    session.finish(now, &outcomes);
    Json(session.view(now)).into_response()
}

async fn admin_log(State(app): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !constant_time_eq(given.as_bytes(), app.admin_token.as_bytes()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(lock(&app.log).clone()).into_response()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A small page that works with no outside files. Everything shown is put in with textContent,
/// so a title or message can never inject markup.
const PAGE: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Verifier</title>
<style>
:root{--bg:#f6f7f9;--card:#fff;--ink:#14171c;--muted:#5b6472;--line:#d9dde4;--accent:#2f5bea;--ok:#1a7f4b;--bad:#c0392b;--warn:#b7791f;--idle:#8a93a1}
@media (prefers-color-scheme:dark){:root{--bg:#0f1216;--card:#171b21;--ink:#e8ebf0;--muted:#9aa4b2;--line:#2a313b;--accent:#6c8cff;--ok:#3ecf8e;--bad:#ff6b5e;--warn:#e6b450;--idle:#6b7686}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.5 system-ui,sans-serif}
main{max-width:640px;margin:0 auto;padding:24px 16px}
h1{font-size:1.4rem;margin:0 0 4px}.sub{color:var(--muted);margin:0 0 20px;font-size:.9rem}
.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:8px 16px;margin-bottom:16px}
.row{display:flex;align-items:center;gap:12px;padding:12px 0;border-bottom:1px solid var(--line)}.row:last-child{border-bottom:0}
.dot{width:12px;height:12px;border-radius:50%;background:var(--idle);flex:none}
.fixed .dot{background:var(--ok)}.failed .dot{background:var(--bad)}.broken .dot{background:var(--warn)}
.t{flex:1}.m{display:block;color:var(--muted);font-size:.85rem}
button{font:inherit;border:0;border-radius:10px;padding:10px 18px;background:var(--accent);color:#fff;cursor:pointer;min-height:44px}
button:disabled{opacity:.55;cursor:not-allowed}.bar{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-bottom:16px}
.flag{border:2px solid var(--ok);border-radius:12px;padding:16px;margin-bottom:16px;background:var(--card)}
.flag code{display:block;word-break:break-all;font-size:1.05rem;margin:8px 0}
.flag p{margin:0;color:var(--muted);font-size:.9rem}.hide{display:none}
button.ghost{background:transparent;color:var(--accent);border:1px solid var(--accent)}
@media (prefers-reduced-motion:no-preference){.dot{transition:background .2s}}
</style></head><body><main>
<h1 id="title">Verifier</h1><p class="sub"><span id="clock"></span></p>
<div class="bar"><button id="go">Verify</button><span class="sub" id="note"></span></div>
<div class="flag hide" id="flag"><strong>Every step is fixed.</strong><code id="flagtext"></code>
<button class="ghost" id="copy">Copy</button><p>Paste this flag into the challenge page.</p></div>
<div class="card" id="list" role="list"></div>
</main><script>
(function(){
var list=document.getElementById('list'),go=document.getElementById('go'),note=document.getElementById('note');
var flag=document.getElementById('flag'),ft=document.getElementById('flagtext'),clock=document.getElementById('clock');
var exp=null,busy=false;
function el(t,c,x){var e=document.createElement(t);if(c)e.className=c;if(x!==undefined)e.textContent=x;return e}
function render(v){
 document.getElementById('title').textContent=v.title||'Verifier';
 exp=v.expires_at;list.replaceChildren();
 v.checks.forEach(function(c){var r=el('div','row '+(c.state||''));r.setAttribute('role','listitem');
  r.appendChild(el('span','dot'));var t=el('span','t',c.title);t.appendChild(el('span','m',c.message));r.appendChild(t);list.appendChild(r)});
 if(v.flag){flag.classList.remove('hide');ft.textContent=v.flag}else{flag.classList.add('hide');ft.textContent=''}
 var wait=v.wait_secs||0;go.disabled=busy||v.running||wait>0;
 note.textContent=v.running?'Running...':(wait>0?'Next run in '+wait+' s':(v.last_run_at?'':'Not run yet'));
}
function tick(){if(exp){var s=Math.max(0,exp-Math.floor(Date.now()/1000));clock.textContent='Time left: '+Math.floor(s/60)+' min '+(s%60)+' s'}}
function load(){fetch('api/state').then(function(r){return r.json()}).then(render).catch(function(){note.textContent='Cannot reach the Verifier'})}
go.addEventListener('click',function(){busy=true;go.disabled=true;note.textContent='Running...';
 fetch('api/verify',{method:'POST'}).then(function(r){return r.json().then(function(b){return {ok:r.ok,b:b}})}).then(function(x){busy=false;if(x.ok)render(x.b);else{load();note.textContent=x.b.error||''}}).catch(function(){busy=false;load()})});
document.getElementById('copy').addEventListener('click',function(){navigator.clipboard&&navigator.clipboard.writeText(ft.textContent)});
setInterval(tick,1000);setInterval(load,5000);load();tick();
})();
</script></body></html>
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicI64, Ordering};
    use tower::ServiceExt;

    struct Fixture {
        app: Router,
        clock: Arc<AtomicI64>,
        fixed: std::path::PathBuf,
        _dir: tempfile::TempDir,
    }

    async fn fixture(flag: Option<&str>) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        // The second check is broken until the marker file exists, like a player's fix.
        let marker = dir.path().join("fixed");
        std::fs::write(
            dir.path().join("verifier.json"),
            r#"{"version":1,"title":"Atlas","min_interval_secs":20,"checks":[
              {"id":"a","title":"First","language":"bash","file":"c.sh","function":"always"},
              {"id":"b","title":"Second","language":"bash","file":"c.sh","function":"marker"}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("c.sh"),
            format!(
                "always() {{ fixed; }}\nmarker() {{ [ -e {} ] && fixed; vulnerable \"not yet\"; }}\n",
                marker.display()
            ),
        )
        .unwrap();
        let loaded = crate::definition::load(dir.path()).unwrap();
        let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib");
        let context = Context::new(&loaded, "10.77.0.42", vec![22], &lib).unwrap();
        let clock = Arc::new(AtomicI64::new(1_000));
        let shared = Arc::clone(&clock);
        let run = RunInfo {
            target_ip: "10.77.0.42".into(),
            target_ports: vec![22],
            flag: flag.map(str::to_owned),
            expires_at: Some(9_999),
            admin_token: "admin-secret-token".into(),
        };
        let app = router(AppState::new(
            loaded,
            context,
            &run,
            Arc::new(move || shared.load(Ordering::SeqCst)),
        ));
        Fixture {
            app,
            clock,
            fixed: marker,
            _dir: dir,
        }
    }

    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let res = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn nothing_has_run_and_the_page_is_hardened() {
        let f = fixture(Some("KYMO{x}")).await;
        let (status, v) = call(&f.app, "GET", "/api/state", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["title"], "Atlas");
        assert_eq!(v["checks"][0]["message"], "Run Verify");
        assert_eq!(v["flag"], Value::Null);
        let res = f
            .app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["cache-control"], "no-store");
        assert!(
            res.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("default-src 'none'")
        );
    }

    #[tokio::test]
    async fn the_flag_shows_only_after_a_fully_green_run_and_is_withdrawn_after_a_regression() {
        let f = fixture(Some("KYMO{x}")).await;
        let (_, first) = call(&f.app, "POST", "/api/verify", None).await;
        assert_eq!(first["checks"][0]["state"], "fixed");
        assert_eq!(first["checks"][1]["state"], "failed");
        assert_eq!(first["checks"][1]["message"], "Weakness still present");
        assert_eq!(first["flag"], Value::Null);
        // Fix it, wait out the interval, verify again.
        std::fs::write(&f.fixed, "").unwrap();
        f.clock.store(1_030, Ordering::SeqCst);
        let (_, second) = call(&f.app, "POST", "/api/verify", None).await;
        assert_eq!(second["flag"], "KYMO{x}");
        // Break it again: the flag disappears on the next run.
        std::fs::remove_file(&f.fixed).unwrap();
        f.clock.store(1_060, Ordering::SeqCst);
        let (_, third) = call(&f.app, "POST", "/api/verify", None).await;
        assert_eq!(third["flag"], Value::Null);
        // The page state shows the same.
        assert_eq!(
            call(&f.app, "GET", "/api/state", None).await.1["flag"],
            Value::Null
        );
    }

    #[tokio::test]
    async fn runs_are_rate_limited_and_say_how_long_to_wait() {
        let f = fixture(None).await;
        assert_eq!(
            call(&f.app, "POST", "/api/verify", None).await.0,
            StatusCode::OK
        );
        f.clock.store(1_005, Ordering::SeqCst);
        let (status, body) = call(&f.app, "POST", "/api/verify", None).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body["wait_secs"], 15);
        f.clock.store(1_021, Ordering::SeqCst);
        assert_eq!(
            call(&f.app, "POST", "/api/verify", None).await.0,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn the_run_log_is_for_the_lab_host_only_and_players_never_see_detail() {
        let f = fixture(Some("KYMO{x}")).await;
        let (_, run) = call(&f.app, "POST", "/api/verify", None).await;
        // The detail line ("not yet") never reaches the page data.
        assert!(!run.to_string().contains("not yet"));
        // The log needs the token; without it the route does not exist.
        assert_eq!(
            call(&f.app, "GET", "/_admin/log", None).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&f.app, "GET", "/_admin/log", Some("wrong")).await.0,
            StatusCode::NOT_FOUND
        );
        let (status, log) = call(&f.app, "GET", "/_admin/log", Some("admin-secret-token")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(log[0]["checks"][1]["detail"], "not yet");
        assert_eq!(log[0]["checks"][1]["state"], "failed");
    }

    #[tokio::test]
    async fn a_second_press_during_a_run_is_refused() {
        let f = fixture(None).await;
        let a = f.app.clone();
        let b = f.app.clone();
        let (x, y) = tokio::join!(
            call(&a, "POST", "/api/verify", None),
            call(&b, "POST", "/api/verify", None)
        );
        let codes = [x.0, y.0];
        assert!(codes.contains(&StatusCode::OK), "{codes:?}");
        assert!(
            codes.contains(&StatusCode::CONFLICT) || codes.contains(&StatusCode::TOO_MANY_REQUESTS),
            "{codes:?}"
        );
    }
}
