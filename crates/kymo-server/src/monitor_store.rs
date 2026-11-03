//! Storage for the request log, rate rules, bans and the audit trail.

use kymo_monitor::{BanKind, Rule, Scope};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::auth::Caller;
use crate::error::ApiError;

/// Request log rows older than this are deleted by the maintenance task.
pub const LOG_RETENTION_SECS: i64 = 7 * 24 * 3600;

// ---------- request log ----------

/// A request log row that owns its text, so it can be queued for the background writer.
#[derive(Debug, Clone)]
pub struct OwnedLog {
    pub at: i64,
    pub method: String,
    pub route: String,
    pub path: String,
    pub status: u16,
    pub ip: String,
    pub user_id: Option<i64>,
    pub ms: i64,
}

/// Writes many rows in one transaction, so a burst of requests costs one commit.
pub async fn log_batch(pool: &SqlitePool, rows: &[OwnedLog]) -> Result<(), ApiError> {
    let mut tx = crate::db::begin_write(pool).await?;
    for e in rows {
        sqlx::query(
            "INSERT INTO request_log (at, method, route, path, status, ip, user_id, ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(e.at)
        .bind(&e.method)
        .bind(&e.route)
        .bind(&e.path)
        .bind(i64::from(e.status))
        .bind(&e.ip)
        .bind(e.user_id)
        .bind(e.ms)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn prune_log(pool: &SqlitePool, before: i64) -> Result<u64, ApiError> {
    Ok(sqlx::query("DELETE FROM request_log WHERE at < ?")
        .bind(before)
        .execute(pool)
        .await?
        .rows_affected())
}

#[derive(Default)]
pub struct LogFilter {
    pub ip: Option<String>,
    pub user_id: Option<i64>,
    pub route: Option<String>,
    pub status: Option<i64>,
    pub since: Option<i64>,
    pub until: Option<i64>,
}

#[derive(Serialize, FromRow)]
pub struct LogRow {
    pub id: i64,
    pub at: i64,
    pub method: String,
    pub route: String,
    pub path: String,
    pub status: i64,
    pub ip: String,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub ms: i64,
}

#[derive(Serialize)]
pub struct LogPage {
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub requests: Vec<LogRow>,
}

const LOG_WHERE: &str = "(?1 IS NULL OR r.ip = ?1) AND (?2 IS NULL OR r.user_id = ?2) AND (?3 IS NULL OR r.route = ?3) AND (?4 IS NULL OR r.status = ?4) AND (?5 IS NULL OR r.at >= ?5) AND (?6 IS NULL OR r.at < ?6)";

pub async fn requests(
    pool: &SqlitePool,
    f: &LogFilter,
    page: i64,
    per_page: i64,
) -> Result<LogPage, ApiError> {
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM request_log r WHERE {LOG_WHERE}"
    ))
    .bind(&f.ip)
    .bind(f.user_id)
    .bind(&f.route)
    .bind(f.status)
    .bind(f.since)
    .bind(f.until)
    .fetch_one(pool)
    .await?;
    let requests = sqlx::query_as(&format!(
        "SELECT r.id, r.at, r.method, r.route, r.path, r.status, r.ip, r.user_id, u.username AS username, r.ms FROM request_log r LEFT JOIN user u ON u.id = r.user_id WHERE {LOG_WHERE} ORDER BY r.id DESC LIMIT ?7 OFFSET ?8"
    ))
    .bind(&f.ip)
    .bind(f.user_id)
    .bind(&f.route)
    .bind(f.status)
    .bind(f.since)
    .bind(f.until)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(pool)
    .await?;
    Ok(LogPage {
        total,
        page,
        per_page,
        requests,
    })
}

#[derive(Serialize, FromRow)]
pub struct Count {
    pub key: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct Summary {
    pub since: i64,
    pub total: i64,
    pub server_errors: i64,
    pub throttled: i64,
    pub forbidden: i64,
    pub top_ips: Vec<Count>,
    pub top_routes: Vec<Count>,
    pub slowest_routes: Vec<Count>,
}

pub async fn summary(pool: &SqlitePool, since: i64) -> Result<Summary, ApiError> {
    let count = |sql: &'static str| async move {
        sqlx::query_scalar::<_, i64>(sql)
            .bind(since)
            .fetch_one(pool)
            .await
    };
    let top = |sql: &'static str| async move {
        sqlx::query_as::<_, Count>(sql)
            .bind(since)
            .fetch_all(pool)
            .await
    };
    Ok(Summary {
        since,
        total: count("SELECT COUNT(*) FROM request_log WHERE at >= ?").await?,
        server_errors: count("SELECT COUNT(*) FROM request_log WHERE at >= ? AND status >= 500")
            .await?,
        throttled: count("SELECT COUNT(*) FROM request_log WHERE at >= ? AND status = 429").await?,
        forbidden: count("SELECT COUNT(*) FROM request_log WHERE at >= ? AND status = 403").await?,
        top_ips: top("SELECT ip AS key, COUNT(*) AS count FROM request_log WHERE at >= ? GROUP BY ip ORDER BY count DESC LIMIT 10").await?,
        top_routes: top("SELECT route AS key, COUNT(*) AS count FROM request_log WHERE at >= ? GROUP BY route ORDER BY count DESC LIMIT 10").await?,
        slowest_routes: top("SELECT route AS key, CAST(AVG(ms) AS INTEGER) AS count FROM request_log WHERE at >= ? GROUP BY route HAVING COUNT(*) >= 5 ORDER BY count DESC LIMIT 10").await?,
    })
}

// ---------- rate rules ----------

#[derive(Serialize, FromRow, Clone)]
pub struct RuleRow {
    pub id: i64,
    pub endpoint: String,
    pub max_requests: i64,
    pub window_secs: i64,
    pub scope: String,
    pub strikes_to_ban: Option<i64>,
    pub ban_secs: i64,
    pub enabled: bool,
    pub created_at: i64,
}

impl RuleRow {
    pub fn rule(&self) -> Option<Rule> {
        Some(Rule {
            id: self.id,
            endpoint: self.endpoint.clone(),
            max_requests: u32::try_from(self.max_requests).ok()?,
            window_secs: self.window_secs,
            scope: Scope::parse(&self.scope)?,
            strikes_to_ban: match self.strikes_to_ban {
                Some(n) => Some(u32::try_from(n).ok()?),
                None => None,
            },
            ban_secs: self.ban_secs,
        })
    }
}

pub async fn rules(pool: &SqlitePool) -> Result<Vec<RuleRow>, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM rate_rule ORDER BY id")
        .fetch_all(pool)
        .await?)
}

pub async fn rule_by_id(pool: &SqlitePool, id: i64) -> Result<RuleRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM rate_rule WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub struct RuleFields<'a> {
    pub endpoint: &'a str,
    pub max_requests: u32,
    pub window_secs: i64,
    pub scope: Scope,
    pub strikes_to_ban: Option<u32>,
    pub ban_secs: i64,
    pub enabled: bool,
}

pub async fn insert_rule(pool: &SqlitePool, f: &RuleFields<'_>, now: i64) -> Result<i64, ApiError> {
    let done = sqlx::query(
        "INSERT INTO rate_rule (endpoint, max_requests, window_secs, scope, strikes_to_ban, ban_secs, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(f.endpoint)
    .bind(i64::from(f.max_requests))
    .bind(f.window_secs)
    .bind(f.scope.as_str())
    .bind(f.strikes_to_ban.map(i64::from))
    .bind(f.ban_secs)
    .bind(f.enabled)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(done.last_insert_rowid())
}

pub async fn update_rule(pool: &SqlitePool, id: i64, f: &RuleFields<'_>) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE rate_rule SET endpoint = ?, max_requests = ?, window_secs = ?, scope = ?, strikes_to_ban = ?, ban_secs = ?, enabled = ? WHERE id = ?",
    )
    .bind(f.endpoint)
    .bind(i64::from(f.max_requests))
    .bind(f.window_secs)
    .bind(f.scope.as_str())
    .bind(f.strikes_to_ban.map(i64::from))
    .bind(f.ban_secs)
    .bind(f.enabled)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_rule(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM rate_rule WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ---------- bans ----------

#[derive(Serialize, FromRow, Clone)]
pub struct BanRow {
    pub id: i64,
    pub kind: String,
    pub subject: String,
    pub reason: String,
    pub auto: bool,
    pub created_by: Option<i64>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub lifted_at: Option<i64>,
    pub lifted_by: Option<i64>,
}

/// Newest first, including lifted and expired bans so the history stays visible.
pub async fn bans(pool: &SqlitePool) -> Result<Vec<BanRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM ban ORDER BY id DESC LIMIT 500")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn ban_by_id(pool: &SqlitePool, id: i64) -> Result<BanRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM ban WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

/// Bans that bind right now.
pub async fn active_bans(pool: &SqlitePool, now: i64) -> Result<Vec<BanRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT * FROM ban WHERE lifted_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(now)
    .fetch_all(pool)
    .await?)
}

pub struct NewBan<'a> {
    pub kind: BanKind,
    pub subject: &'a str,
    pub reason: &'a str,
    pub auto: bool,
    pub created_by: Option<i64>,
    pub now: i64,
    pub expires_at: Option<i64>,
}

pub async fn insert_ban(pool: &SqlitePool, b: &NewBan<'_>) -> Result<i64, ApiError> {
    let done = sqlx::query(
        "INSERT INTO ban (kind, subject, reason, auto, created_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(b.kind.as_str())
    .bind(b.subject)
    .bind(b.reason)
    .bind(b.auto)
    .bind(b.created_by)
    .bind(b.now)
    .bind(b.expires_at)
    .execute(pool)
    .await?;
    Ok(done.last_insert_rowid())
}

pub async fn lift_ban(
    pool: &SqlitePool,
    id: i64,
    now: i64,
    by: Option<i64>,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE ban SET lifted_at = ?, lifted_by = ? WHERE id = ? AND lifted_at IS NULL")
        .bind(now)
        .bind(by)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ---------- audit ----------

#[derive(Serialize, FromRow)]
pub struct AuditRow {
    pub id: i64,
    pub at: i64,
    pub actor_user_id: Option<i64>,
    pub actor_name: Option<String>,
    pub actor_role: String,
    pub action: String,
    pub target_kind: String,
    pub target_id: String,
    pub detail: String,
}

#[derive(Serialize)]
pub struct AuditPage {
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub entries: Vec<AuditRow>,
}

/// Records one moderation action. `actor` is none for actions the platform takes itself.
pub async fn audit(
    pool: &SqlitePool,
    now: i64,
    actor: Option<&Caller>,
    action: &str,
    target_kind: &str,
    target_id: impl ToString,
    detail: &str,
) -> Result<(), ApiError> {
    let (user, role) = match actor {
        Some(c) => (c.user_id, c.role.as_str()),
        None => (None, "system"),
    };
    sqlx::query(
        "INSERT INTO audit (at, actor_user_id, actor_role, action, target_kind, target_id, detail) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(now)
    .bind(user)
    .bind(role)
    .bind(action)
    .bind(target_kind)
    .bind(target_id.to_string())
    .bind(detail.chars().take(500).collect::<String>())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn audit_page(
    pool: &SqlitePool,
    action: Option<&str>,
    actor: Option<i64>,
    page: i64,
    per_page: i64,
) -> Result<AuditPage, ApiError> {
    const FILTER: &str = "(?1 IS NULL OR a.action = ?1) AND (?2 IS NULL OR a.actor_user_id = ?2)";
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM audit a WHERE {FILTER}"))
        .bind(action)
        .bind(actor)
        .fetch_one(pool)
        .await?;
    let entries = sqlx::query_as(&format!(
        "SELECT a.id, a.at, a.actor_user_id, u.username AS actor_name, a.actor_role, a.action, a.target_kind, a.target_id, a.detail FROM audit a LEFT JOIN user u ON u.id = a.actor_user_id WHERE {FILTER} ORDER BY a.id DESC LIMIT ?3 OFFSET ?4"
    ))
    .bind(action)
    .bind(actor)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(pool)
    .await?;
    Ok(AuditPage {
        total,
        page,
        per_page,
        entries,
    })
}
