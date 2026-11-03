//! Storage of challenge instance runs. Rules live in kymo-instances.

use kymo_instances::{InstanceRecord, InstanceState};
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;
use crate::jeopardy_store::Scorer;

#[derive(Debug, Clone, FromRow)]
pub struct InstanceRow {
    pub id: i64,
    pub event_id: i64,
    pub challenge_id: i64,
    pub scorer_kind: String,
    pub scorer_id: i64,
    pub user_id: i64,
    pub handle: Option<String>,
    pub host_port: Option<i64>,
    pub lab_ip: Option<String>,
    pub failure: Option<String>,
    /// The lab host it runs on; none is this machine's own runtime.
    pub lab_host_id: Option<i64>,
    /// The name the platform built for it before asking for it to be started.
    pub name: Option<String>,
    /// The address players connect to, as it was when the run started.
    pub public_host: Option<String>,
    pub state: String,
    pub started_at: i64,
    pub expires_at: i64,
    pub stopped_at: Option<i64>,
}

impl InstanceRow {
    pub fn record(&self) -> Result<InstanceRecord, ApiError> {
        Ok(InstanceRecord {
            state: InstanceState::parse(&self.state).ok_or(ApiError::Internal)?,
            started_at: self.started_at,
            expires_at: self.expires_at,
        })
    }
}

/// The participant's newest run on a challenge, whatever its state.
pub async fn latest(
    pool: &SqlitePool,
    challenge_id: i64,
    scorer: Scorer,
) -> Result<Option<InstanceRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT * FROM instance WHERE challenge_id = ? AND scorer_kind = ? AND scorer_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(challenge_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .fetch_optional(pool)
    .await?)
}

/// Runs that count against the start budget: everything except failed starts.
pub async fn starts_used(
    pool: &SqlitePool,
    challenge_id: i64,
    scorer: Scorer,
) -> Result<u32, ApiError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM instance WHERE challenge_id = ? AND scorer_kind = ? AND scorer_id = ? AND state <> 'failed'",
    )
    .bind(challenge_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .fetch_one(pool)
    .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Live runs of this participant in the event on challenges other than `challenge_id`.
/// Runs past their deadline do not count, even before the reaper has stopped them.
pub async fn live_elsewhere(
    pool: &SqlitePool,
    event_id: i64,
    challenge_id: i64,
    scorer: Scorer,
    now: i64,
) -> Result<u32, ApiError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM instance WHERE event_id = ? AND challenge_id <> ? AND scorer_kind = ? AND scorer_id = ? AND state IN ('starting', 'running') AND expires_at > ?",
    )
    .bind(event_id)
    .bind(challenge_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

pub struct NewRun<'a> {
    pub event_id: i64,
    pub challenge_id: i64,
    pub scorer: Scorer,
    pub user_id: i64,
    pub started_at: i64,
    pub expires_at: i64,
    pub state: &'a str,
    /// The address inside the player's lab, for VPN mode events.
    pub lab_ip: Option<&'a str>,
    pub lab_host_id: Option<i64>,
    pub name: &'a str,
    pub public_host: &'a str,
}

pub async fn insert(pool: &SqlitePool, run: &NewRun<'_>) -> Result<i64, ApiError> {
    let done = sqlx::query(
        "INSERT INTO instance (event_id, challenge_id, scorer_kind, scorer_id, user_id, state, started_at, expires_at, lab_ip, lab_host_id, name, public_host) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(run.event_id)
    .bind(run.challenge_id)
    .bind(run.scorer.kind())
    .bind(run.scorer.id())
    .bind(run.user_id)
    .bind(run.state)
    .bind(run.started_at)
    .bind(run.expires_at)
    .bind(run.lab_ip)
    .bind(run.lab_host_id)
    .bind(run.name)
    .bind(run.public_host)
    .execute(pool)
    .await?;
    Ok(done.last_insert_rowid())
}

/// The container exists: remember its handle and port. The run stays `starting` until the
/// service answers.
pub async fn mark_started(
    pool: &SqlitePool,
    id: i64,
    handle: &str,
    host_port: u16,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE instance SET handle = ?, host_port = ? WHERE id = ?")
        .bind(handle)
        .bind(i64::from(host_port))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The service answers: only now does the player get an address.
pub async fn mark_ready(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query("UPDATE instance SET state = 'running' WHERE id = ? AND state = 'starting'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Ends a run as failed and says why.
pub async fn mark_failed(
    pool: &SqlitePool,
    id: i64,
    now: i64,
    reason: &str,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE instance SET state = 'failed', stopped_at = ?, failure = ? WHERE id = ? AND state IN ('starting', 'running')")
        .bind(now)
        .bind(reason)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Ends a run: `state` is stopped, expired or failed.
pub async fn mark_ended(
    pool: &SqlitePool,
    id: i64,
    state: InstanceState,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE instance SET state = ?, stopped_at = ? WHERE id = ?")
        .bind(state.as_str())
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn by_id(pool: &SqlitePool, id: i64) -> Result<InstanceRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM instance WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub async fn live(pool: &SqlitePool) -> Result<Vec<InstanceRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM instance WHERE state IN ('starting', 'running')")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_for_event(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<Vec<InstanceRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM instance WHERE event_id = ? ORDER BY id DESC LIMIT 500")
            .bind(event_id)
            .fetch_all(pool)
            .await?,
    )
}

/// A player's live instances in one event, with the challenge each belongs to.
#[derive(Debug, Clone, FromRow)]
pub struct ListRow {
    pub id: i64,
    pub challenge_id: i64,
    pub title: String,
    pub state: String,
    pub lab_ip: Option<String>,
    pub public_host: Option<String>,
    pub host_port: Option<i64>,
    pub started_at: i64,
    pub expires_at: i64,
}

pub async fn live_of(
    pool: &SqlitePool,
    event_id: i64,
    scorer: Scorer,
) -> Result<Vec<ListRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT i.id, i.challenge_id, c.title, i.state, i.lab_ip, i.public_host, i.host_port, i.started_at, i.expires_at FROM instance i JOIN challenge c ON c.id = i.challenge_id WHERE i.event_id = ? AND i.scorer_kind = ? AND i.scorer_id = ? AND i.state IN ('starting', 'running') ORDER BY i.id DESC",
    )
    .bind(event_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .fetch_all(pool)
    .await?)
}
