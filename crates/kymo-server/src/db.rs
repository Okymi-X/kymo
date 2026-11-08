use std::str::FromStr;

use kymo_engine::{Bracket, Format};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;
use crate::event_store;
use crate::tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum Status {
    Draft,
    Live,
    Done,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Progress {
    pub played: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Team {
    pub id: i64,
    pub name: String,
    pub seed: i64,
    pub score: i64,
    /// The account that plays for this team in a solo event.
    pub user_id: Option<i64>,
    /// The player team that plays for this team in a team event.
    pub player_team_id: Option<i64>,
}

/// How a tournament decides its matches with challenges (see ARCHITECTURE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ChallengeSettings {
    pub assignment: kymo_duel::Assignment,
    pub solo: u8,
    pub team: u8,
    pub seed: i64,
    /// Seconds a match stays open once it starts, or none for no limit.
    pub time_limit: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
struct Row {
    id: i64,
    name: String,
    slug: String,
    format: String,
    third_place: bool,
    groups: i64,
    advance: i64,
    rounds: i64,
    best_of: i64,
    challenge_assignment: String,
    challenges_solo: i64,
    challenges_team: i64,
    challenge_seed: i64,
    challenge_time_limit: Option<i64>,
    event_state: Option<String>,
    description: String,
    accent: Option<String>,
    event_id: Option<i64>,
    status: Status,
    bracket: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tournament {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub format: Format,
    pub third_place: bool,
    pub groups: u8,
    pub advance: u8,
    /// Swiss rounds, zero to pick the usual count at start.
    pub rounds: u8,
    /// Games per match: 1, 3, 5 or 7.
    pub best_of: u8,
    pub challenges: ChallengeSettings,
    /// True while the organizer has paused the tournament's event, so no page shows it as live.
    pub paused: bool,
    /// Rules, venue or any note the organizer wants players to read.
    pub description: String,
    /// Optional #rrggbb accent that tints this tournament's pages.
    pub accent: Option<String>,
    /// The platform event this tournament belongs to.
    pub event_id: Option<i64>,
    pub status: Status,
    pub created_at: String,
    pub teams: Vec<Team>,
    /// Full roster size, kept when the public list trims `teams`.
    pub team_count: usize,
    pub bracket: Option<Bracket>,
    pub champion: Option<u32>,
    pub progress: Progress,
}

pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .foreign_keys(true)
        .create_if_missing(true);
    let max = if url.contains(":memory:") { 1 } else { 5 };
    let pool = SqlitePoolOptions::new()
        .max_connections(max)
        .connect_with(options)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Starts a transaction that will write. `BEGIN IMMEDIATE` takes the write lock at once, so when
/// another connection is writing (the request log writer, a background job) this waits for it
/// like `busy_timeout` promises. A plain `BEGIN` that reads first and writes later is refused
/// with "database is locked" the moment the lock is taken, without waiting.
pub async fn begin_write(
    pool: &SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

fn format_to_text(format: Format) -> &'static str {
    match format {
        Format::SingleElimination => "single_elimination",
        Format::DoubleElimination => "double_elimination",
        Format::RoundRobin => "round_robin",
        Format::GroupKnockout => "group_knockout",
        Format::Swiss => "swiss",
    }
}

fn hydrate(row: Row, teams: Vec<Team>) -> Result<Tournament, ApiError> {
    let parse = |e: serde_json::Error| {
        tracing::error!(%e, "corrupt stored tournament");
        ApiError::Internal
    };
    let bracket: Option<Bracket> = row
        .bracket
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(parse)?;
    Ok(Tournament {
        champion: bracket.as_ref().and_then(Bracket::champion),
        progress: bracket.as_ref().map_or(
            Progress {
                played: 0,
                total: 0,
            },
            |b| {
                let (played, total) = b.progress();
                Progress { played, total }
            },
        ),
        bracket,
        id: row.id,
        name: row.name,
        slug: row.slug,
        format: serde_json::from_value(row.format.into()).map_err(parse)?,
        third_place: row.third_place,
        groups: u8::try_from(row.groups).unwrap_or(2),
        advance: u8::try_from(row.advance).unwrap_or(2),
        rounds: u8::try_from(row.rounds).unwrap_or(0),
        best_of: u8::try_from(row.best_of).unwrap_or(1),
        paused: row.event_state.as_deref() == Some("paused"),
        challenges: ChallengeSettings {
            assignment: kymo_duel::Assignment::parse(&row.challenge_assignment).unwrap_or_default(),
            solo: u8::try_from(row.challenges_solo).unwrap_or(1),
            team: u8::try_from(row.challenges_team).unwrap_or(3),
            seed: row.challenge_seed,
            time_limit: row.challenge_time_limit,
        },
        description: row.description,
        accent: row.accent,
        event_id: row.event_id,
        status: row.status,
        created_at: row.created_at,
        team_count: teams.len(),
        teams,
    })
}

async fn teams_of(pool: &SqlitePool, id: i64) -> Result<Vec<Team>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, name, seed, score, user_id, player_team_id FROM team WHERE tournament_id = ? ORDER BY seed",
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Tournament>, ApiError> {
    let rows: Vec<Row> = sqlx::query_as("SELECT *, (SELECT e.state FROM event e WHERE e.id = tournament.event_id) AS event_state FROM tournament ORDER BY id DESC")
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let teams = teams_of(pool, row.id).await?;
        out.push(hydrate(row, teams)?);
    }
    Ok(out)
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Tournament, ApiError> {
    let row: Row = sqlx::query_as("SELECT *, (SELECT e.state FROM event e WHERE e.id = tournament.event_id) AS event_state FROM tournament WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    let teams = teams_of(pool, row.id).await?;
    hydrate(row, teams)
}

pub async fn get_by_slug(pool: &SqlitePool, slug: &str) -> Result<Tournament, ApiError> {
    let row: Row = sqlx::query_as("SELECT *, (SELECT e.state FROM event e WHERE e.id = tournament.event_id) AS event_state FROM tournament WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await?;
    let teams = teams_of(pool, row.id).await?;
    hydrate(row, teams)
}

pub struct NewTournament<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    pub format: Format,
    pub third_place: bool,
    pub groups: u8,
    pub advance: u8,
    pub rounds: u8,
    pub best_of: u8,
}

/// Creates the tournament and its platform event in one transaction.
pub async fn create(pool: &SqlitePool, new: NewTournament<'_>) -> Result<i64, ApiError> {
    let mut tx = begin_write(pool).await?;
    let event_id = event_store::insert(
        &mut tx,
        &event_store::tournament_fields(new.name),
        new.slug,
        tokens::now(),
    )
    .await?;
    let result = sqlx::query(
        "INSERT INTO tournament (name, slug, format, third_place, groups, advance, rounds, best_of, event_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new.name)
    .bind(new.slug)
    .bind(format_to_text(new.format))
    .bind(new.third_place)
    .bind(i64::from(new.groups))
    .bind(i64::from(new.advance))
    .bind(i64::from(new.rounds))
    .bind(i64::from(new.best_of))
    .bind(event_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.last_insert_rowid())
}

pub async fn set_accent(pool: &SqlitePool, id: i64, accent: Option<&str>) -> Result<(), ApiError> {
    let done = sqlx::query("UPDATE tournament SET accent = ? WHERE id = ?")
        .bind(accent)
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn rename(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    description: Option<&str>,
) -> Result<(), ApiError> {
    let done = sqlx::query(
        "UPDATE tournament SET name = ?, description = COALESCE(?, description) WHERE id = ?",
    )
    .bind(name)
    .bind(description)
    .bind(id)
    .execute(pool)
    .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    // One name and description, shown the same on the event and on the tournament.
    sqlx::query("UPDATE event SET name = ?, description = COALESCE(?, description) WHERE id = (SELECT event_id FROM tournament WHERE id = ?)")
        .bind(name)
        .bind(description)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let mut tx = begin_write(pool).await?;
    let event_id: Option<Option<i64>> =
        sqlx::query_scalar("SELECT event_id FROM tournament WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(event_id) = event_id else {
        return Err(ApiError::NotFound);
    };
    sqlx::query("DELETE FROM tournament WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some(event_id) = event_id {
        sqlx::query("DELETE FROM event WHERE id = ?")
            .bind(event_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Default)]
pub struct NewTeam {
    pub name: String,
    pub score: i64,
    pub user_id: Option<i64>,
    pub player_team_id: Option<i64>,
}

/// Replaces the whole roster; list order defines the seeds.
pub async fn replace_teams(pool: &SqlitePool, id: i64, teams: &[NewTeam]) -> Result<(), ApiError> {
    let mut tx = begin_write(pool).await?;
    sqlx::query("DELETE FROM team WHERE tournament_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (index, team) in teams.iter().enumerate() {
        sqlx::query("INSERT INTO team (tournament_id, name, seed, score, user_id, player_team_id) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(id)
            .bind(&team.name)
            .bind(index as i64 + 1)
            .bind(team.score)
            .bind(team.user_id)
            .bind(team.player_team_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn save_bracket(
    pool: &SqlitePool,
    id: i64,
    bracket: Option<&Bracket>,
    status: Status,
    at: i64,
) -> Result<(), ApiError> {
    let json = bracket
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| ApiError::Internal)?;
    sqlx::query("UPDATE tournament SET bracket = ?, status = ? WHERE id = ?")
        .bind(json)
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    // The tournament's event follows it, so the events list and Operations do not show a
    // finished tournament as a draft.
    // The event follows the tournament, but never undoes what an organizer decided about the
    // event itself: a paused event stays paused, an ended or archived one stays so, and a draft
    // that is taking registrations keeps taking them.
    sqlx::query(
        "UPDATE event SET state = CASE \
             WHEN state = 'archived' THEN 'archived' \
             WHEN state = 'paused' AND ?1 = 'live' THEN 'paused' \
             WHEN state = 'ended' AND ?1 = 'live' THEN 'ended' \
             WHEN state = 'registration_open' AND ?1 = 'draft' THEN 'registration_open' \
             WHEN ?1 = 'live' THEN 'live' \
             WHEN ?1 = 'done' THEN 'ended' \
             ELSE 'draft' END, \
         ended_at = CASE \
             WHEN ?1 = 'done' THEN COALESCE(ended_at, ?2) \
             WHEN state IN ('ended', 'archived') THEN ended_at \
             ELSE NULL END, \
         started_at = CASE \
             WHEN ?1 = 'live' THEN COALESCE(started_at, ?2) \
             WHEN ?1 = 'draft' THEN NULL \
             ELSE started_at END \
         WHERE id = (SELECT event_id FROM tournament WHERE id = ?3)",
    )
    .bind(status)
    .bind(at)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
