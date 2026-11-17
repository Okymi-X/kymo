//! Request monitoring middleware: ban checks, per endpoint rate rules, and the request log.
//! The rules themselves live in kymo-monitor; this module loads them, applies them to each
//! API request and writes what happened. Staff (anyone who may use the monitor) are never
//! limited or banned here, so a bad rule cannot lock the organizers out.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use axum::extract::{MatchedPath, OriginalUri, Request, State};
use axum::http::header::{AUTHORIZATION, USER_AGENT};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use kymo_core::Permission;
use kymo_monitor::{BanKind, Decision, Limiter, Rule, Scope, expiry, is_active};
use sqlx::SqlitePool;
use tokio::sync::{OnceCell, mpsc, oneshot};

use crate::auth::client_ip;
use crate::error::ApiError;
use crate::monitor_store::{self as store, NewBan, OwnedLog};
use crate::state::AppState;
use crate::{tokens, users};

const UNMATCHED: &str = "unmatched";
const MONITOR_PREFIX: &str = "/api/v1/admin/monitor";
/// Load balancer probes would drown the log.
const HEALTH_ROUTE: &str = "/api/v1/health";

#[derive(Default)]
struct Inner {
    limiter: Limiter,
    rules: Vec<Rule>,
    bans: HashMap<(BanKind, String), Option<i64>>,
}

/// In memory view of the rules and active bans, loaded lazily from the database and reloaded
/// after every change made through the API.
#[derive(Default)]
pub struct Monitor {
    inner: Mutex<Inner>,
    loaded: OnceCell<()>,
    log: std::sync::OnceLock<mpsc::Sender<LogMessage>>,
}

/// Rows waiting for the background writer, and a way to wait until they are all written.
enum LogMessage {
    Row(OwnedLog),
    Flush(oneshot::Sender<()>),
}

const LOG_QUEUE: usize = 8192;
const LOG_BATCH: usize = 100;

/// Writes queued request rows in batches, so no response ever waits for its own log row.
async fn log_writer(pool: SqlitePool, mut rx: mpsc::Receiver<LogMessage>) {
    let mut rows: Vec<OwnedLog> = Vec::new();
    while let Some(first) = rx.recv().await {
        let mut waiters = Vec::new();
        let mut take = |message: LogMessage, rows: &mut Vec<OwnedLog>| match message {
            LogMessage::Row(row) => rows.push(row),
            LogMessage::Flush(done) => waiters.push(done),
        };
        take(first, &mut rows);
        while rows.len() < LOG_BATCH {
            match rx.try_recv() {
                Ok(message) => take(message, &mut rows),
                Err(_) => break,
            }
        }
        if !rows.is_empty() {
            if let Err(e) = store::log_batch(&pool, &rows).await {
                tracing::warn!("could not write {} request log rows: {e:?}", rows.len());
            }
            rows.clear();
        }
        for done in waiters {
            let _ = done.send(());
        }
    }
}

impl Monitor {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queues a request row for the background writer. When the queue is full the row is dropped
    /// rather than slowing the response down: the log is for people to look at, not a ledger.
    pub fn record(&self, pool: &SqlitePool, row: OwnedLog) {
        let sender = self.log.get_or_init(|| {
            let (tx, rx) = mpsc::channel(LOG_QUEUE);
            tokio::spawn(log_writer(pool.clone(), rx));
            tx
        });
        if sender.try_send(LogMessage::Row(row)).is_err() {
            tracing::warn!("the request log queue is full, a row was dropped");
        }
    }

    /// Waits until every row queued so far is in the database. Readers of the log call this
    /// first so what they show is current.
    pub async fn flush(&self) {
        let Some(sender) = self.log.get() else {
            return;
        };
        let (done, wait) = oneshot::channel();
        if sender.send(LogMessage::Flush(done)).await.is_ok() {
            let _ = wait.await;
        }
    }

    fn banned(&self, kind: BanKind, subject: &str, now: i64) -> bool {
        self.lock()
            .bans
            .get(&(kind, subject.to_owned()))
            .is_some_and(|expires| is_active(*expires, now))
    }

    fn remember_ban(&self, kind: BanKind, subject: &str, expires_at: Option<i64>) {
        self.lock()
            .bans
            .insert((kind, subject.to_owned()), expires_at);
    }
}

async fn load(state: &AppState) -> Result<(), ApiError> {
    let now = state.now();
    let rules: Vec<Rule> = store::rules(&state.pool)
        .await?
        .iter()
        .filter(|r| r.enabled)
        .filter_map(store::RuleRow::rule)
        .filter(|rule| rule.validate().is_ok())
        .collect();
    let mut bans = HashMap::new();
    for ban in store::active_bans(&state.pool, now).await? {
        if let Some(kind) = BanKind::parse(&ban.kind) {
            bans.insert((kind, ban.subject), ban.expires_at);
        }
    }
    let mut inner = state.monitor().lock();
    inner.rules = rules;
    inner.bans = bans;
    Ok(())
}

async fn ensure_loaded(state: &AppState) -> Result<(), ApiError> {
    state
        .monitor()
        .loaded
        .get_or_try_init(|| load(state))
        .await
        .map(|_| ())
}

/// Call after any change to rules or bans so the next request sees it.
pub async fn reload(state: &AppState) -> Result<(), ApiError> {
    load(state).await?;
    // Mark loaded in case the first request has not happened yet.
    let _ = state.monitor().loaded.set(());
    Ok(())
}

enum Verdict {
    Pass,
    Throttle(i64),
    Ban {
        rule: Rule,
        subject: String,
        duration_secs: Option<i64>,
    },
}

async fn gate(
    state: &AppState,
    ip: std::net::IpAddr,
    user_id: Option<i64>,
    method: &str,
    route: &str,
    now: i64,
) -> Result<(), ApiError> {
    if let Err(e) = ensure_loaded(state).await {
        tracing::warn!("monitor could not load its rules: {e:?}");
        return Ok(());
    }
    let monitor = state.monitor();
    let ip_text = ip.to_string();
    if monitor.banned(BanKind::Ip, &ip_text, now) {
        return Err(ApiError::AddressBanned);
    }
    if let Some(id) = user_id
        && monitor.banned(BanKind::User, &id.to_string(), now)
    {
        return Err(ApiError::Banned);
    }
    let verdict = {
        let mut inner = monitor.lock();
        let rules: Vec<Rule> = inner
            .rules
            .iter()
            .filter(|r| r.matches(method, route))
            .cloned()
            .collect();
        let mut verdict = Verdict::Pass;
        for rule in rules {
            let subject = match rule.scope {
                Scope::Ip => Some(ip_text.clone()),
                Scope::User => user_id.map(|id| id.to_string()),
            };
            let Some(subject) = subject else { continue };
            // An address we cannot tell apart from every other caller is never banned, so
            // it is judged by a rule that only throttles. Banning would reset the counters
            // and let the next request through.
            let rule = if rule.scope == Scope::Ip && ip.is_unspecified() {
                Rule {
                    strikes_to_ban: None,
                    ..rule
                }
            } else {
                rule
            };
            match inner.limiter.check(&rule, &subject, now) {
                Decision::Allow => {}
                Decision::Throttle { retry_after_secs } => {
                    verdict = Verdict::Throttle(retry_after_secs);
                    break;
                }
                Decision::Ban { duration_secs } => {
                    verdict = Verdict::Ban {
                        rule,
                        subject,
                        duration_secs,
                    };
                    break;
                }
            }
        }
        verdict
    };
    match verdict {
        Verdict::Pass => Ok(()),
        Verdict::Throttle(retry_after) => Err(ApiError::Throttled { retry_after }),
        Verdict::Ban {
            rule,
            subject,
            duration_secs,
        } => auto_ban(state, &rule, &subject, duration_secs, now).await,
    }
}

async fn auto_ban(
    state: &AppState,
    rule: &Rule,
    subject: &str,
    duration_secs: Option<i64>,
    now: i64,
) -> Result<(), ApiError> {
    let kind = match rule.scope {
        Scope::Ip => BanKind::Ip,
        Scope::User => BanKind::User,
    };
    let expires_at = expiry(now, duration_secs);
    let reason = format!("rate rule exceeded: {}", rule.endpoint);
    let recorded = async {
        store::insert_ban(
            &state.pool,
            &NewBan {
                kind,
                subject,
                reason: &reason,
                auto: true,
                created_by: None,
                now,
                expires_at,
            },
        )
        .await?;
        if kind == BanKind::User
            && let Ok(id) = subject.parse::<i64>()
        {
            users::moderate(&state.pool, id, None, Some(true)).await?;
        }
        store::audit(
            &state.pool,
            now,
            None,
            "auto_ban",
            kind.as_str(),
            subject,
            &reason,
        )
        .await
    }
    .await;
    if let Err(e) = recorded {
        tracing::warn!("could not record an automatic ban: {e:?}");
    }
    state.monitor().remember_ban(kind, subject, expires_at);
    if kind == BanKind::User {
        crate::vpn::sync_quiet(state).await;
    }
    Err(match kind {
        BanKind::Ip => ApiError::AddressBanned,
        BanKind::User => ApiError::Banned,
    })
}

/// Who is calling, from the bearer token if there is a valid one. The flag says whether the
/// caller is staff and exempt from limits and bans.
async fn identify(state: &AppState, secret: Option<String>) -> (Option<i64>, bool) {
    let Some(secret) = secret else {
        return (None, false);
    };
    match tokens::access(&state.pool, &secret).await {
        Ok(Some(access)) => (
            access.user.map(|u| u.id),
            state
                .can(access.role(), Permission::Monitor)
                .await
                .unwrap_or(false),
        ),
        _ => (None, false),
    }
}

pub async fn layer(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let now = state.now();
    let ip = client_ip(
        request.headers(),
        request.extensions(),
        state.trusts_proxy(),
    );
    let method = request.method().as_str().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| UNMATCHED.to_owned(), |m| m.as_str().to_owned());
    // The router is nested under /api/v1, so the request URI has lost that prefix.
    let path: String = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |original| original.0.path())
        .chars()
        .take(200)
        .collect();
    let query: String = request
        .uri()
        .query()
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect();
    let ua: String = request
        .headers()
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect();
    let secret = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);
    let (user_id, staff) = identify(&state, secret).await;

    let blocked = if staff {
        None
    } else {
        gate(&state, ip, user_id, &method, &route, now).await.err()
    };
    let response = match blocked {
        Some(error) => error.into_response(),
        None => next.run(request).await,
    };

    if !route.starts_with(MONITOR_PREFIX) && route != HEALTH_ROUTE {
        state.monitor().record(
            &state.pool,
            OwnedLog {
                at: now,
                method,
                route,
                path,
                status: response.status().as_u16(),
                ip: ip.to_string(),
                user_id,
                ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
                query,
                ua,
                staff,
            },
        );
    }
    response
}

/// Deletes request log rows past their retention. Run by the maintenance task.
pub async fn prune(state: &AppState) -> Result<u64, ApiError> {
    state.monitor().flush().await;
    store::prune_log(&state.pool, state.now() - store::LOG_RETENTION_SECS).await
}
