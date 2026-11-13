//! Attack alerts and the live graphs of the monitor: the detector settings the organizer edits,
//! the alerts the detectors raise, automatic bans, and the bucketed numbers behind the charts.

use std::collections::BTreeMap;

use kymo_monitor::{BanKind, DetectorConfig, DetectorKind, Sample, default_config, evaluate};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;
use crate::monitor_store::{self as store, NewBan};
use crate::state::AppState;
use crate::users;

/// Requests read per run; a flood past this is still caught, because the newest are kept.
const MAX_SAMPLES: i64 = 100_000;

// ---------- detector settings ----------

#[derive(FromRow)]
struct DetectorRow {
    kind: String,
    enabled: bool,
    alert_at: i64,
    ban_at: Option<i64>,
    window_secs: i64,
    ban_secs: Option<i64>,
}

/// The settings in force for every detector: the organizer's change, or the built in default.
pub async fn configs(
    pool: &SqlitePool,
) -> Result<BTreeMap<DetectorKind, DetectorConfig>, ApiError> {
    let mut out: BTreeMap<DetectorKind, DetectorConfig> = DetectorKind::ALL
        .into_iter()
        .map(|k| (k, default_config(k)))
        .collect();
    let rows: Vec<DetectorRow> = sqlx::query_as(
        "SELECT kind, enabled, alert_at, ban_at, window_secs, ban_secs FROM detector",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        let Some(kind) = DetectorKind::parse(&row.kind) else {
            continue;
        };
        let parsed = (|| {
            Some(DetectorConfig {
                enabled: row.enabled,
                alert_at: u32::try_from(row.alert_at).ok()?,
                ban_at: match row.ban_at {
                    Some(n) => Some(u32::try_from(n).ok()?),
                    None => None,
                },
                window_secs: row.window_secs,
                ban_secs: row.ban_secs,
            })
        })();
        // A damaged row falls back to the default instead of switching the detector off.
        if let Some(config) = parsed.filter(|c| c.validate().is_ok()) {
            out.insert(kind, config);
        }
    }
    Ok(out)
}

pub async fn save_config(
    pool: &SqlitePool,
    kind: DetectorKind,
    config: &DetectorConfig,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO detector (kind, enabled, alert_at, ban_at, window_secs, ban_secs, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(kind) DO UPDATE SET enabled = excluded.enabled, alert_at = excluded.alert_at, ban_at = excluded.ban_at, window_secs = excluded.window_secs, ban_secs = excluded.ban_secs, updated_at = excluded.updated_at")
        .bind(kind.as_str())
        .bind(config.enabled)
        .bind(i64::from(config.alert_at))
        .bind(config.ban_at.map(i64::from))
        .bind(config.window_secs)
        .bind(config.ban_secs)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn reset_config(pool: &SqlitePool, kind: DetectorKind) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM detector WHERE kind = ?")
        .bind(kind.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Serialize)]
pub struct DetectorOut {
    pub kind: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub config: DetectorConfig,
    pub default: DetectorConfig,
    pub customized: bool,
}

pub async fn detector_list(pool: &SqlitePool) -> Result<Vec<DetectorOut>, ApiError> {
    let current = configs(pool).await?;
    Ok(DetectorKind::ALL
        .into_iter()
        .map(|kind| {
            let default = default_config(kind);
            let config = current.get(&kind).copied().unwrap_or(default);
            DetectorOut {
                kind: kind.as_str(),
                label: kind.label(),
                description: kind.describe(),
                config,
                default,
                customized: config != default,
            }
        })
        .collect())
}

// ---------- alerts ----------

#[derive(FromRow)]
struct AlertDb {
    id: i64,
    kind: String,
    user_id: Option<i64>,
    username: Option<String>,
    ip: String,
    count: i64,
    first_at: i64,
    last_at: i64,
    evidence: String,
    status: String,
    note: String,
    ban_id: Option<i64>,
    handled_at: Option<i64>,
}

#[derive(Serialize)]
pub struct AlertOut {
    pub id: i64,
    pub kind: String,
    pub label: &'static str,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub ip: String,
    pub count: i64,
    pub first_at: i64,
    pub last_at: i64,
    pub evidence: Vec<String>,
    pub status: String,
    pub note: String,
    pub ban_id: Option<i64>,
    pub handled_at: Option<i64>,
    /// `high` once the automatic ban level was reached, `medium` for a signed in player below it,
    /// `low` for a visitor known only by address.
    pub severity: &'static str,
}

fn out(row: AlertDb, configs: &BTreeMap<DetectorKind, DetectorConfig>) -> AlertOut {
    let kind = DetectorKind::parse(&row.kind);
    let peak = kind
        .and_then(|k| configs.get(&k))
        .and_then(|c| c.ban_at)
        .is_some_and(|b| row.count >= i64::from(b));
    let severity = if peak || row.status == "banned" {
        "high"
    } else if row.user_id.is_some() {
        "medium"
    } else {
        "low"
    };
    AlertOut {
        id: row.id,
        label: kind.map_or("Unknown", DetectorKind::label),
        kind: row.kind,
        user_id: row.user_id,
        username: row.username,
        ip: row.ip,
        count: row.count,
        first_at: row.first_at,
        last_at: row.last_at,
        evidence: serde_json::from_str(&row.evidence).unwrap_or_default(),
        status: row.status,
        note: row.note,
        ban_id: row.ban_id,
        handled_at: row.handled_at,
        severity,
    }
}

const ALERT_SELECT: &str = "SELECT a.id, a.kind, a.user_id, u.username AS username, a.ip, a.count, a.first_at, a.last_at, a.evidence, a.status, a.note, a.ban_id, a.handled_at FROM alert a LEFT JOIN user u ON u.id = a.user_id";

#[derive(Serialize)]
pub struct AlertPage {
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    /// Open alerts about signed in players, for the badge.
    pub open_players: i64,
    /// Every open alert, visitors included.
    pub open_all: i64,
    pub alerts: Vec<AlertOut>,
}

pub struct AlertFilter {
    /// `open`, `handled` or anything else for all.
    pub status: String,
    /// Only alerts about signed in players.
    pub players_only: bool,
}

pub async fn alerts(
    pool: &SqlitePool,
    f: &AlertFilter,
    page: i64,
    per_page: i64,
) -> Result<AlertPage, ApiError> {
    let status = match f.status.as_str() {
        "open" => "a.status = 'open'",
        "handled" => "a.status != 'open'",
        _ => "1 = 1",
    };
    let who = if f.players_only {
        "a.user_id IS NOT NULL"
    } else {
        "1 = 1"
    };
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM alert a WHERE {status} AND {who}"
    ))
    .fetch_one(pool)
    .await?;
    let rows: Vec<AlertDb> = sqlx::query_as(&format!(
        "{ALERT_SELECT} WHERE {status} AND {who} ORDER BY (a.status = 'open') DESC, a.last_at DESC, a.id DESC LIMIT ? OFFSET ?"
    ))
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(pool)
    .await?;
    let current = configs(pool).await?;
    let open_players: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert WHERE status = 'open' AND user_id IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    let open_all: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert WHERE status = 'open'")
        .fetch_one(pool)
        .await?;
    Ok(AlertPage {
        total,
        page,
        per_page,
        open_players,
        open_all,
        alerts: rows.into_iter().map(|r| out(r, &current)).collect(),
    })
}

pub async fn alert_by_id(pool: &SqlitePool, id: i64) -> Result<AlertOut, ApiError> {
    let row: AlertDb = sqlx::query_as(&format!("{ALERT_SELECT} WHERE a.id = ?"))
        .bind(id)
        .fetch_one(pool)
        .await?;
    let current = configs(pool).await?;
    Ok(out(row, &current))
}

/// Marks an alert handled: dismissed with a note, or banned with the ban that was made.
pub async fn close_alert(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    note: &str,
    by: Option<i64>,
    ban_id: Option<i64>,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE alert SET status = ?, note = ?, handled_by = ?, handled_at = ?, ban_id = COALESCE(?, ban_id) WHERE id = ?")
        .bind(status)
        .bind(note)
        .bind(by)
        .bind(now)
        .bind(ban_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ---------- detection ----------

pub async fn recent_samples(pool: &SqlitePool, since: i64) -> Result<Vec<Sample>, ApiError> {
    #[derive(FromRow)]
    struct Row {
        at: i64,
        method: String,
        route: String,
        path: String,
        query: String,
        status: i64,
        ip: String,
        user_id: Option<i64>,
        ua: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT at, method, route, path, query, status, ip, user_id, ua FROM request_log WHERE at >= ? AND staff = 0 ORDER BY at DESC LIMIT ?",
    )
    .bind(since)
    .bind(MAX_SAMPLES)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Sample {
            at: r.at,
            method: r.method,
            route: r.route,
            path: r.path,
            query: r.query,
            status: u16::try_from(r.status).unwrap_or(0),
            ip: r.ip,
            user_id: r.user_id,
            ua: r.ua,
        })
        .collect())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DetectionReport {
    pub raised: usize,
    pub updated: usize,
    pub banned: usize,
    pub unbanned: usize,
}

/// Lifts the account flag of a player whose temporary ban has run out. Until now the flag stayed
/// set for good, which made every temporary ban of a player permanent.
async fn expire_user_bans(state: &AppState, now: i64) -> Result<usize, ApiError> {
    let due: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, subject FROM ban WHERE kind = 'user' AND lifted_at IS NULL AND expires_at IS NOT NULL AND expires_at <= ?",
    )
    .bind(now)
    .fetch_all(&state.pool)
    .await?;
    let mut lifted = 0;
    for (id, subject) in due {
        sqlx::query("UPDATE ban SET lifted_at = ? WHERE id = ? AND lifted_at IS NULL")
            .bind(now)
            .bind(id)
            .execute(&state.pool)
            .await?;
        // Another ban on the same player that still binds keeps the account blocked.
        let still: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ban WHERE kind = 'user' AND subject = ? AND lifted_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
        )
        .bind(&subject)
        .bind(now)
        .fetch_one(&state.pool)
        .await?;
        if still == 0
            && let Ok(user_id) = subject.parse::<i64>()
        {
            let _ = users::moderate(&state.pool, user_id, None, Some(false)).await;
        }
        lifted += 1;
    }
    if lifted > 0 {
        crate::monitor::reload(state).await?;
        crate::vpn::sync_quiet(state).await;
    }
    Ok(lifted)
}

/// One pass: end expired bans, run every detector over the recent requests, raise or grow alerts,
/// and ban automatically a player who reached a detector's ban level.
pub async fn run_once(state: &AppState) -> Result<DetectionReport, ApiError> {
    state.monitor().flush().await;
    let now = state.now();
    let mut report = DetectionReport {
        unbanned: expire_user_bans(state, now).await?,
        ..DetectionReport::default()
    };
    let configs = configs(&state.pool).await?;
    let widest = configs
        .values()
        .filter(|c| c.enabled)
        .map(|c| c.window_secs)
        .max()
        .unwrap_or(0);
    if widest == 0 {
        return Ok(report);
    }
    let samples = recent_samples(&state.pool, now - widest).await?;
    if samples.is_empty() {
        return Ok(report);
    }
    for kind in DetectorKind::ALL {
        let config = configs
            .get(&kind)
            .copied()
            .unwrap_or_else(|| default_config(kind));
        for hit in evaluate(kind, &config, &samples, now) {
            let handled: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(handled_at) FROM alert WHERE kind = ? AND user_id IS ? AND (user_id IS NOT NULL OR ip = ?) AND status != 'open'",
            )
            .bind(kind.as_str())
            .bind(hit.user_id)
            .bind(&hit.ip)
            .fetch_one(&state.pool)
            .await?;
            // Behaviour an organizer already looked at is not raised again.
            if handled.is_some_and(|at| hit.last_at <= at) {
                continue;
            }
            let evidence = serde_json::to_string(&hit.evidence).map_err(|_| ApiError::Internal)?;
            let open: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM alert WHERE kind = ? AND user_id IS ? AND (user_id IS NOT NULL OR ip = ?) AND status = 'open'",
            )
            .bind(kind.as_str())
            .bind(hit.user_id)
            .bind(&hit.ip)
            .fetch_optional(&state.pool)
            .await?;
            let id = match open {
                Some(id) => {
                    sqlx::query("UPDATE alert SET count = MAX(count, ?), last_at = MAX(last_at, ?), ip = ?, evidence = ? WHERE id = ?")
                        .bind(i64::from(hit.count))
                        .bind(hit.last_at)
                        .bind(&hit.ip)
                        .bind(&evidence)
                        .bind(id)
                        .execute(&state.pool)
                        .await?;
                    report.updated += 1;
                    id
                }
                None => {
                    let done = sqlx::query("INSERT INTO alert (kind, user_id, ip, count, first_at, last_at, evidence) VALUES (?, ?, ?, ?, ?, ?, ?)")
                        .bind(kind.as_str())
                        .bind(hit.user_id)
                        .bind(&hit.ip)
                        .bind(i64::from(hit.count))
                        .bind(hit.first_at)
                        .bind(hit.last_at)
                        .bind(&evidence)
                        .execute(&state.pool)
                        .await?;
                    report.raised += 1;
                    done.last_insert_rowid()
                }
            };
            // Only a signed in player is banned automatically. An address can be everyone behind
            // one network, so addresses are left to the organizer.
            let (Some(user_id), Some(ban_at)) = (hit.user_id, config.ban_at) else {
                continue;
            };
            if hit.count < ban_at
                || !auto_ban(state, id, user_id, kind, &config, hit.count, now).await?
            {
                continue;
            }
            report.banned += 1;
        }
    }
    Ok(report)
}

/// Bans a player account automatically and ends their sessions. None when they are staff or
/// already banned, otherwise the id of the ban made.
pub async fn ban_player(
    state: &AppState,
    user_id: i64,
    reason: &str,
    expires_at: Option<i64>,
    now: i64,
) -> Result<Option<i64>, ApiError> {
    let Ok(target) = users::by_id(&state.pool, user_id).await else {
        return Ok(None);
    };
    let Ok(role) = target.role.parse::<kymo_core::Role>() else {
        return Ok(None);
    };
    if target.banned || state.can(role, kymo_core::Permission::Monitor).await? {
        return Ok(None);
    }
    let subject = user_id.to_string();
    let ban_id = store::insert_ban(
        &state.pool,
        &NewBan {
            kind: BanKind::User,
            subject: &subject,
            reason,
            auto: true,
            created_by: None,
            now,
            expires_at,
        },
    )
    .await?;
    users::moderate(&state.pool, user_id, None, Some(true)).await?;
    store::audit(&state.pool, now, None, "auto_ban", "user", &subject, reason).await?;
    crate::monitor::reload(state).await?;
    crate::vpn::sync_quiet(state).await;
    Ok(Some(ban_id))
}

/// Bans the player for the detector's duration. False when they are staff or already banned.
async fn auto_ban(
    state: &AppState,
    alert_id: i64,
    user_id: i64,
    kind: DetectorKind,
    config: &DetectorConfig,
    count: u32,
    now: i64,
) -> Result<bool, ApiError> {
    let reason = format!("{} ({count} counted): banned automatically", kind.label());
    let expires_at = config.ban_secs.map(|s| now + s);
    let Some(ban_id) = ban_player(state, user_id, &reason, expires_at, now).await? else {
        return Ok(false);
    };
    close_alert(
        &state.pool,
        alert_id,
        "banned",
        "banned automatically",
        None,
        Some(ban_id),
        now,
    )
    .await?;
    Ok(true)
}

// ---------- graphs ----------

#[derive(Serialize, FromRow, Clone, Default)]
pub struct Bucket {
    pub t: i64,
    pub total: i64,
    pub client: i64,
    pub server: i64,
    pub throttled: i64,
    pub avg_ms: i64,
    pub max_ms: i64,
    pub ips: i64,
    pub users: i64,
    pub login_ok: i64,
    pub login_failed: i64,
    pub signups: i64,
    pub submissions: i64,
    pub instance_starts: i64,
}

#[derive(Serialize, FromRow)]
pub struct Marker {
    pub id: i64,
    pub kind: String,
    pub user_id: Option<i64>,
    pub last_at: i64,
    pub status: String,
}

#[derive(Serialize)]
pub struct Series {
    pub from: i64,
    pub to: i64,
    pub step: i64,
    pub buckets: Vec<Bucket>,
    pub markers: Vec<Marker>,
}

/// Bucket width for a range, chosen so a chart has between about 60 and 170 columns.
pub fn step_for(minutes: i64) -> i64 {
    if minutes <= 15 {
        15
    } else if minutes <= 60 {
        60
    } else if minutes <= 1440 {
        900
    } else {
        3600
    }
}

const SERIES_SQL: &str = "SELECT (at / ?1) * ?1 AS t, COUNT(*) AS total, \
SUM(CASE WHEN status >= 400 AND status < 500 AND status != 429 THEN 1 ELSE 0 END) AS client, \
SUM(CASE WHEN status >= 500 THEN 1 ELSE 0 END) AS server, \
SUM(CASE WHEN status = 429 THEN 1 ELSE 0 END) AS throttled, \
CAST(AVG(ms) AS INTEGER) AS avg_ms, MAX(ms) AS max_ms, \
COUNT(DISTINCT ip) AS ips, COUNT(DISTINCT user_id) AS users, \
SUM(CASE WHEN method = 'POST' AND route IN ('/api/v1/auth/player-login', '/api/v1/auth/login') AND status = 200 THEN 1 ELSE 0 END) AS login_ok, \
SUM(CASE WHEN method = 'POST' AND route IN ('/api/v1/auth/player-login', '/api/v1/auth/login') AND status = 401 THEN 1 ELSE 0 END) AS login_failed, \
SUM(CASE WHEN method = 'POST' AND route = '/api/v1/auth/register' AND status = 201 THEN 1 ELSE 0 END) AS signups, \
SUM(CASE WHEN method = 'POST' AND route LIKE '%/submit' THEN 1 ELSE 0 END) AS submissions, \
SUM(CASE WHEN method = 'POST' AND route LIKE '%/instance' AND status = 201 THEN 1 ELSE 0 END) AS instance_starts \
FROM request_log WHERE at >= ?2 GROUP BY t ORDER BY t";

pub async fn series(pool: &SqlitePool, now: i64, minutes: i64) -> Result<Series, ApiError> {
    let step = step_for(minutes);
    let from = ((now - minutes * 60) / step) * step;
    let to = (now / step) * step;
    let found: Vec<Bucket> = sqlx::query_as(SERIES_SQL)
        .bind(step)
        .bind(from)
        .fetch_all(pool)
        .await?;
    // Every bucket in the range is there, empty ones as zeros, so a chart has no gaps to guess.
    let mut by_time: BTreeMap<i64, Bucket> = found.into_iter().map(|b| (b.t, b)).collect();
    let buckets = (0..=((to - from) / step))
        .map(|i| {
            let t = from + i * step;
            by_time.remove(&t).unwrap_or(Bucket {
                t,
                ..Bucket::default()
            })
        })
        .collect();
    let markers = sqlx::query_as(
        "SELECT id, kind, user_id, last_at, status FROM alert WHERE last_at >= ? ORDER BY last_at",
    )
    .bind(from)
    .fetch_all(pool)
    .await?;
    Ok(Series {
        from,
        to,
        step,
        buckets,
        markers,
    })
}

#[derive(Serialize)]
pub struct Hot {
    pub key: String,
    pub label: String,
    pub user_id: Option<i64>,
    pub count: i64,
    /// Requests per minute over the range, oldest first.
    pub trend: Vec<i64>,
}

#[derive(Serialize)]
pub struct HotLists {
    pub minutes: i64,
    pub addresses: Vec<Hot>,
    pub players: Vec<Hot>,
}

async fn trend(
    pool: &SqlitePool,
    filter: &str,
    bind_text: Option<&str>,
    bind_id: Option<i64>,
    from: i64,
    minutes: i64,
) -> Result<Vec<i64>, ApiError> {
    let sql = format!(
        "SELECT (at / 60) * 60 AS t, COUNT(*) AS n FROM request_log WHERE at >= ? AND {filter} GROUP BY t"
    );
    let mut query = sqlx::query_as::<_, (i64, i64)>(&sql).bind(from);
    if let Some(text) = bind_text {
        query = query.bind(text.to_owned());
    }
    if let Some(id) = bind_id {
        query = query.bind(id);
    }
    let found: BTreeMap<i64, i64> = query.fetch_all(pool).await?.into_iter().collect();
    let start = (from / 60) * 60;
    Ok((0..minutes)
        .map(|i| found.get(&(start + i * 60)).copied().unwrap_or(0))
        .collect())
}

pub async fn hot(pool: &SqlitePool, now: i64, minutes: i64) -> Result<HotLists, ApiError> {
    let from = now - minutes * 60;
    let addresses: Vec<(String, i64)> = sqlx::query_as(
        "SELECT ip, COUNT(*) AS n FROM request_log WHERE at >= ? GROUP BY ip ORDER BY n DESC LIMIT 8",
    )
    .bind(from)
    .fetch_all(pool)
    .await?;
    let mut hot_addresses = Vec::new();
    for (ip, count) in addresses {
        let trend = trend(pool, "ip = ?", Some(&ip), None, from, minutes).await?;
        hot_addresses.push(Hot {
            key: ip.clone(),
            label: ip,
            user_id: None,
            count,
            trend,
        });
    }
    let players: Vec<(i64, Option<String>, i64)> = sqlx::query_as(
        "SELECT r.user_id, u.username, COUNT(*) AS n FROM request_log r LEFT JOIN user u ON u.id = r.user_id WHERE r.at >= ? AND r.user_id IS NOT NULL GROUP BY r.user_id ORDER BY n DESC LIMIT 8",
    )
    .bind(from)
    .fetch_all(pool)
    .await?;
    let mut hot_players = Vec::new();
    for (id, name, count) in players {
        let trend = trend(pool, "user_id = ?", None, Some(id), from, minutes).await?;
        hot_players.push(Hot {
            key: id.to_string(),
            label: name.unwrap_or_else(|| format!("#{id}")),
            user_id: Some(id),
            count,
            trend,
        });
    }
    Ok(HotLists {
        minutes,
        addresses: hot_addresses,
        players: hot_players,
    })
}

#[derive(Serialize)]
pub struct Heat {
    /// Start of each hour, unix seconds, oldest first, for the last seven days.
    pub from: i64,
    pub hours: Vec<i64>,
}

pub async fn heatmap(pool: &SqlitePool, now: i64) -> Result<Heat, ApiError> {
    let end = (now / 3600) * 3600;
    let from = end - 167 * 3600;
    let found: BTreeMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
        "SELECT (at / 3600) * 3600 AS h, COUNT(*) FROM request_log WHERE at >= ? GROUP BY h",
    )
    .bind(from)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    Ok(Heat {
        from,
        hours: (0..168)
            .map(|i| found.get(&(from + i * 3600)).copied().unwrap_or(0))
            .collect(),
    })
}

/// One subject's requests per minute around an alert, for the small chart in its details.
pub async fn subject_trend(
    pool: &SqlitePool,
    user_id: Option<i64>,
    ip: &str,
    from: i64,
    to: i64,
) -> Result<Vec<(i64, i64)>, ApiError> {
    let found: Vec<(i64, i64)> = match user_id {
        Some(id) => {
            sqlx::query_as("SELECT (at / 60) * 60 AS t, COUNT(*) FROM request_log WHERE user_id = ? AND at BETWEEN ? AND ? GROUP BY t ORDER BY t")
                .bind(id)
                .bind(from)
                .bind(to)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query_as("SELECT (at / 60) * 60 AS t, COUNT(*) FROM request_log WHERE ip = ? AND user_id IS NULL AND at BETWEEN ? AND ? GROUP BY t ORDER BY t")
                .bind(ip)
                .bind(from)
                .bind(to)
                .fetch_all(pool)
                .await?
        }
    };
    Ok(found)
}
