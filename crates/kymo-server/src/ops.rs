//! Operations: health report, consistent database snapshots, scheduled backups with
//! retention, and the housekeeping that keeps the database small. Snapshots use SQLite's
//! `VACUUM INTO`, which copies a consistent view while the server keeps serving requests.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::error::ApiError;
use crate::state::AppState;
use crate::{event_store, instance_store, instances, monitor};

const HEALTH_WINDOW_SECS: i64 = 900;
const BACKUP_PREFIX: &str = "kymo-";
const BACKUP_SUFFIX: &str = ".db";

/// Where scheduled backups go and how many to keep.
#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub dir: PathBuf,
    pub every_secs: i64,
    pub keep: usize,
}

/// The outcome of the last run of a background job.
#[derive(Debug, Clone, Serialize)]
pub struct RunInfo {
    pub at: i64,
    pub detail: String,
    pub error: Option<String>,
}

#[derive(Default)]
struct Jobs {
    reaper: Option<RunInfo>,
    backup: Option<RunInfo>,
    maintenance: Option<RunInfo>,
}

/// Status of background jobs, kept in memory for the health report.
pub struct Ops {
    started: Instant,
    jobs: Mutex<Jobs>,
}

impl Default for Ops {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            jobs: Mutex::default(),
        }
    }
}

impl Ops {
    fn jobs(&self) -> MutexGuard<'_, Jobs> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn record_reaper(&self, at: i64, result: &Result<usize, ApiError>) {
        self.jobs().reaper = Some(run_info(at, result.as_ref().map(|n| format!("{n} ended"))));
    }

    fn record_backup(&self, at: i64, result: Result<String, String>) {
        self.jobs().backup = Some(match result {
            Ok(name) => RunInfo {
                at,
                detail: name,
                error: None,
            },
            Err(e) => RunInfo {
                at,
                detail: String::new(),
                error: Some(e),
            },
        });
    }

    fn record_maintenance(&self, at: i64, report: &CleanupReport) {
        self.jobs().maintenance = Some(RunInfo {
            at,
            detail: format!(
                "{} log rows, {} sessions removed",
                report.log_rows, report.expired_sessions
            ),
            error: None,
        });
    }
}

fn run_info(at: i64, result: Result<String, &ApiError>) -> RunInfo {
    match result {
        Ok(detail) => RunInfo {
            at,
            detail,
            error: None,
        },
        Err(e) => RunInfo {
            at,
            detail: String::new(),
            error: Some(format!("{e:?}")),
        },
    }
}

pub fn uptime_secs(state: &AppState) -> u64 {
    state.ops().started.elapsed().as_secs()
}

// ---------- snapshots and backups ----------

/// Copies a consistent view of the database to `path`, which must not exist yet.
pub async fn snapshot_to(pool: &SqlitePool, path: &Path) -> Result<(), ApiError> {
    let target = path.to_str().ok_or(ApiError::Internal)?;
    sqlx::query("VACUUM INTO ?")
        .bind(target)
        .execute(pool)
        .await?;
    // SQLite quietly writes nothing for an in memory database; never report that as a backup.
    if !path.is_file() {
        tracing::error!("snapshot: no file was written, is the database in memory?");
        return Err(ApiError::Internal);
    }
    Ok(())
}

/// A snapshot as bytes. The temporary directory is private to this process and removed on
/// return, so the copy (which holds password hashes) is never readable by other users.
pub async fn snapshot_bytes(pool: &SqlitePool) -> Result<Vec<u8>, ApiError> {
    let dir = tempfile::tempdir().map_err(|e| {
        tracing::error!("snapshot: {e}");
        ApiError::Internal
    })?;
    let file = dir.path().join("snapshot.db");
    snapshot_to(pool, &file).await?;
    tokio::fs::read(&file).await.map_err(|e| {
        tracing::error!("snapshot: {e}");
        ApiError::Internal
    })
}

fn backup_name(now: i64) -> String {
    format!("{BACKUP_PREFIX}{now:012}{BACKUP_SUFFIX}")
}

fn is_backup_name(name: &str) -> bool {
    name.strip_prefix(BACKUP_PREFIX)
        .and_then(|rest| rest.strip_suffix(BACKUP_SUFFIX))
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupFile {
    pub name: String,
    pub size: u64,
}

/// Backup files in the configured directory, newest first.
pub async fn list_backups(config: &BackupConfig) -> Result<Vec<BackupFile>, ApiError> {
    let mut found = Vec::new();
    let mut entries = match tokio::fs::read_dir(&config.dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => {
            tracing::error!("listing backups: {e}");
            return Err(ApiError::Internal);
        }
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_backup_name(&name)
            && let Ok(meta) = entry.metadata().await
        {
            found.push(BackupFile {
                name,
                size: meta.len(),
            });
        }
    }
    found.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(found)
}

/// Writes a snapshot into the backup directory (created private on first use) and deletes
/// the oldest files beyond the retention count.
pub async fn write_backup(state: &AppState) -> Result<BackupFile, ApiError> {
    let config = state.backup_config().ok_or_else(|| {
        ApiError::Conflict("backups are not configured, set KYMO_BACKUP_DIR".into())
    })?;
    let now = state.now();
    let result = write_backup_inner(state, config, now).await;
    state.ops().record_backup(
        now,
        match &result {
            Ok(file) => Ok(file.name.clone()),
            Err(e) => Err(format!("{e:?}")),
        },
    );
    result
}

async fn write_backup_inner(
    state: &AppState,
    config: &BackupConfig,
    now: i64,
) -> Result<BackupFile, ApiError> {
    let io = |e: std::io::Error| {
        tracing::error!("backup: {e}");
        ApiError::Internal
    };
    create_private_dir(&config.dir).map_err(io)?;
    let name = backup_name(now);
    let path = config.dir.join(&name);
    if path.exists() {
        return Err(ApiError::Conflict(
            "a backup was already written this second".into(),
        ));
    }
    snapshot_to(&state.pool, &path).await?;
    restrict_file(&path).map_err(io)?;
    let size = tokio::fs::metadata(&path).await.map_err(io)?.len();
    for old in list_backups(config).await?.into_iter().skip(config.keep) {
        let _ = tokio::fs::remove_file(config.dir.join(old.name)).await;
    }
    Ok(BackupFile { name, size })
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_file(_: &Path) -> std::io::Result<()> {
    Ok(())
}

// ---------- housekeeping ----------

#[derive(Debug, Default, Serialize)]
pub struct CleanupReport {
    pub instances_ended: usize,
    pub log_rows: u64,
    pub expired_sessions: u64,
}

pub async fn cleanup(state: &AppState) -> Result<CleanupReport, ApiError> {
    let instances_ended = instances::reap(state).await?;
    let log_rows = monitor::prune(state).await?;
    let expired_sessions =
        sqlx::query("DELETE FROM token WHERE expires_at IS NOT NULL AND expires_at <= ?")
            .bind(state.now())
            .execute(&state.pool)
            .await?
            .rows_affected();
    Ok(CleanupReport {
        instances_ended,
        log_rows,
        expired_sessions,
    })
}

/// The periodic job: cleanup, then a scheduled backup when one is due.
pub async fn maintain(state: &AppState) -> Result<(), ApiError> {
    // Heals drift: a restarted gateway, a lab network lost with the container platform.
    crate::vpn::sync_quiet(state).await;
    let report = cleanup(state).await?;
    state.ops().record_maintenance(state.now(), &report);
    if let Some(config) = state.backup_config() {
        let newest = list_backups(config).await?.into_iter().next();
        let due = newest.is_none_or(|file| {
            file.name
                .strip_prefix(BACKUP_PREFIX)
                .and_then(|r| r.strip_suffix(BACKUP_SUFFIX))
                .and_then(|d| d.parse::<i64>().ok())
                .is_none_or(|at| state.now() - at >= config.every_secs)
        });
        if due {
            write_backup(state).await?;
        }
    }
    Ok(())
}

// ---------- health ----------

async fn scalar(pool: &SqlitePool, sql: &str, bind: i64) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(sql).bind(bind).fetch_one(pool).await?)
}

/// Whether the database answers.
pub async fn database_ok(pool: &SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok()
}

pub async fn report(state: &AppState) -> Result<Value, ApiError> {
    state.monitor().flush().await;
    let now = state.now();
    let pool = &state.pool;
    let started = Instant::now();
    let db_ok = database_ok(pool).await;
    let ping_ms = started.elapsed().as_millis() as u64;
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await?;

    let since = now - HEALTH_WINDOW_SECS;
    let requests = scalar(
        pool,
        "SELECT COUNT(*) FROM request_log WHERE at >= ?",
        since,
    )
    .await?;
    let server_errors = scalar(
        pool,
        "SELECT COUNT(*) FROM request_log WHERE at >= ? AND status >= 500",
        since,
    )
    .await?;
    let throttled = scalar(
        pool,
        "SELECT COUNT(*) FROM request_log WHERE at >= ? AND status = 429",
        since,
    )
    .await?;
    let avg_ms = scalar(
        pool,
        "SELECT COALESCE(CAST(AVG(ms) AS INTEGER), 0) FROM request_log WHERE at >= ?",
        since,
    )
    .await?;
    let recent_errors: Vec<Value> = sqlx::query_as::<_, (i64, String, String, i64, i64)>(
        "SELECT at, method, route, status, ms FROM request_log WHERE status >= 500 ORDER BY id DESC LIMIT 20",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(at, method, route, status, ms)| json!({ "at": at, "method": method, "route": route, "status": status, "ms": ms }))
    .collect();

    let live = instance_store::live(pool).await?;
    let expired_pending = live.iter().filter(|r| now >= r.expires_at).count();
    let (runtime_ok, orphans) = match state.instance_setup() {
        None => (None, 0),
        Some(setup) => match setup.runtime.list_managed().await {
            Ok(managed) => {
                let owned: std::collections::HashSet<String> = live
                    .iter()
                    .filter(|r| r.lab_host_id.is_none())
                    .map(|r| r.id.to_string())
                    .collect();
                let orphans = managed
                    .iter()
                    .filter(|m| !owned.contains(&m.instance_id))
                    .count();
                (Some(true), orphans)
            }
            Err(_) => (Some(false), 0),
        },
    };

    // Lab hosts: what the last health check (run by the reaper every 30 seconds) found.
    let lab_rows = crate::lab_store::list(pool).await?;
    let mut lab_list = Vec::new();
    for row in &lab_rows {
        let (_, live_here) = crate::lab_store::counts(pool, row.id).await?;
        let reachable = row
            .enabled
            .then(|| row.last_error.is_none() && row.last_ok_at.is_some());
        lab_list.push(json!({
            "id": row.id,
            "name": row.name,
            "enabled": row.enabled,
            "reachable": reachable,
            "last_checked_at": row.last_checked_at,
            "last_error": row.last_error,
            "live_instances": live_here,
        }));
    }
    let labs_failed = lab_rows
        .iter()
        .any(|r| r.enabled && r.last_checked_at.is_some() && r.last_error.is_some());

    let vpn = match state.vpn_setup() {
        None => json!({ "enabled": false }),
        Some(setup) => {
            let labs = crate::vpn_store::labs(pool).await?.len();
            let clients = crate::vpn_store::clients(pool).await?.len();
            match setup.gateway.status().await {
                Ok(s) => {
                    json!({ "enabled": true, "gateway_ok": s.running, "labs": labs, "clients": clients, "error": null })
                }
                Err(e) => {
                    json!({ "enabled": true, "gateway_ok": false, "labs": labs, "clients": clients, "error": e.to_string() })
                }
            }
        }
    };
    let vpn_failed = vpn["enabled"] == true && vpn["gateway_ok"] == false;

    let mut events = Vec::new();
    for event in event_store::list(pool, false).await? {
        if event.state == "archived" {
            continue;
        }
        let live_instances = live.iter().filter(|r| r.event_id == event.id).count();
        let submissions = if event.kind == "jeopardy" {
            scalar(
                pool,
                "SELECT COUNT(*) FROM submission WHERE at >= ? AND event_id = ?",
                now - 600,
            )
            .await
            .unwrap_or(0)
        } else {
            0
        };
        events.push(json!({
            "id": event.id,
            "slug": event.slug,
            "name": event.name,
            "kind": event.kind,
            "state": event.state,
            "effective_state": event.effective_state(now)?.as_str(),
            "participants": event_store::participant_count(pool, event.id).await?,
            "live_instances": live_instances,
            "submissions_last_10m": submissions,
        }));
    }

    let jobs = state.ops().jobs();
    let reaper_failed = jobs.reaper.as_ref().is_some_and(|j| j.error.is_some());
    let backup_failed = jobs.backup.as_ref().is_some_and(|j| j.error.is_some());
    let jobs_json = json!({
        "reaper": jobs.reaper,
        "backup": jobs.backup,
        "maintenance": jobs.maintenance,
    });
    drop(jobs);

    let degraded = !db_ok
        || runtime_ok == Some(false)
        || reaper_failed
        || backup_failed
        || vpn_failed
        || labs_failed;
    Ok(json!({
        "status": if degraded { "degraded" } else { "ok" },
        "time": now,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": uptime_secs(state),
        "database": { "ok": db_ok, "ping_ms": ping_ms, "size_bytes": pages * page_size },
        "traffic": {
            "window_secs": HEALTH_WINDOW_SECS,
            "requests": requests,
            "server_errors": server_errors,
            "throttled": throttled,
            "avg_ms": avg_ms,
        },
        "labs": lab_list,
        "instances": {
            "enabled": state.instance_setup().is_some(),
            "runtime_ok": runtime_ok,
            "live": live.len(),
            "expired_pending": expired_pending,
            "orphans": orphans,
        },
        "backups": { "configured": state.backup_config().is_some() },
        "vpn": vpn,
        "jobs": jobs_json,
        "events": events,
        "recent_errors": recent_errors,
    }))
}
