//! Persistence for challenge-based tournament matches: DFIR items, assignments, answers and
//! the open and finalized state of a match. Rules live in `kymo-duel`.

use kymo_jeopardy::FlagSpec;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::db::ChallengeSettings;
use crate::error::ApiError;

#[derive(Debug, Clone, FromRow)]
pub struct ItemRow {
    pub id: i64,
    pub challenge_id: i64,
    pub position: i64,
    pub question: String,
    #[sqlx(json)]
    pub answer: FlagSpec,
    pub case_sensitive: bool,
    pub points: i64,
    pub max_attempts: Option<i64>,
}

pub struct ItemFields {
    pub question: String,
    pub answer: FlagSpec,
    pub case_sensitive: bool,
    pub points: i64,
    pub max_attempts: Option<i64>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct AnswerRow {
    pub id: i64,
    pub challenge_id: i64,
    pub item_id: Option<i64>,
    pub side_team_id: i64,
    pub user_id: i64,
    pub verdict: String,
    pub points: i64,
    pub value: String,
    pub ip: String,
    pub at: i64,
}

#[derive(Debug, Clone, Copy, FromRow)]
pub struct MatchState {
    pub opened_at: Option<i64>,
    pub finalized_at: Option<i64>,
}

pub async fn save_settings(
    pool: &SqlitePool,
    tournament_id: i64,
    s: &ChallengeSettings,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE tournament SET challenge_assignment = ?, challenges_solo = ?, challenges_team = ?, challenge_seed = ?, challenge_time_limit = ? WHERE id = ?")
        .bind(s.assignment.as_str())
        .bind(i64::from(s.solo))
        .bind(i64::from(s.team))
        .bind(s.seed)
        .bind(s.time_limit)
        .bind(tournament_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn items(pool: &SqlitePool, challenge_id: i64) -> Result<Vec<ItemRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM dfir_item WHERE challenge_id = ? ORDER BY position, id")
            .bind(challenge_id)
            .fetch_all(pool)
            .await?,
    )
}

/// Replaces all items. Ids change, so this is refused once any answer refers to the challenge.
pub async fn replace_items(
    pool: &SqlitePool,
    challenge_id: i64,
    items: &[ItemFields],
) -> Result<(), ApiError> {
    let answered: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM duel_answer WHERE challenge_id = ?")
            .bind(challenge_id)
            .fetch_one(pool)
            .await?;
    if answered > 0 {
        return Err(ApiError::Conflict(
            "players have already answered this challenge, so its questions can no longer change"
                .into(),
        ));
    }
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query("DELETE FROM dfir_item WHERE challenge_id = ?")
        .bind(challenge_id)
        .execute(&mut *tx)
        .await?;
    for (position, item) in items.iter().enumerate() {
        sqlx::query("INSERT INTO dfir_item (challenge_id, position, question, answer, case_sensitive, points, max_attempts) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(challenge_id)
            .bind(position as i64)
            .bind(&item.question)
            .bind(serde_json::to_string(&item.answer).map_err(|_| ApiError::Internal)?)
            .bind(item.case_sensitive)
            .bind(item.points)
            .bind(item.max_attempts)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn assigned(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
) -> Result<Vec<i64>, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT challenge_id FROM duel_assignment WHERE tournament_id = ? AND match_id = ? ORDER BY position, challenge_id",
    )
    .bind(tournament_id)
    .bind(match_id)
    .fetch_all(pool)
    .await?)
}

/// Every (match, challenge) pair of a tournament.
pub async fn all_assigned(
    pool: &SqlitePool,
    tournament_id: i64,
) -> Result<Vec<(i64, i64)>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT match_id, challenge_id FROM duel_assignment WHERE tournament_id = ? ORDER BY match_id, position",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?)
}

pub async fn answer_count(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM duel_answer WHERE tournament_id = ? AND match_id = ?",
    )
    .bind(tournament_id)
    .bind(match_id)
    .fetch_one(pool)
    .await?)
}

/// Sets the match's challenges. Refused once someone has answered.
pub async fn assign(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
    challenge_ids: &[i64],
) -> Result<(), ApiError> {
    if answer_count(pool, tournament_id, match_id).await? > 0 {
        return Err(ApiError::Conflict(
            "this match has answers already, so its challenges can no longer change".into(),
        ));
    }
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query("DELETE FROM duel_assignment WHERE tournament_id = ? AND match_id = ?")
        .bind(tournament_id)
        .bind(match_id)
        .execute(&mut *tx)
        .await?;
    for (position, id) in challenge_ids.iter().enumerate() {
        sqlx::query("INSERT INTO duel_assignment (tournament_id, match_id, challenge_id, position) VALUES (?, ?, ?, ?)")
            .bind(tournament_id)
            .bind(match_id)
            .bind(id)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn answers(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
) -> Result<Vec<AnswerRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, challenge_id, item_id, side_team_id, user_id, verdict, points, value, ip, at FROM duel_answer WHERE tournament_id = ? AND match_id = ? ORDER BY id",
    )
    .bind(tournament_id)
    .bind(match_id)
    .fetch_all(pool)
    .await?)
}

pub struct NewAnswer<'a> {
    pub tournament_id: i64,
    pub match_id: i64,
    pub challenge_id: i64,
    pub item_id: Option<i64>,
    pub side_team_id: i64,
    pub user_id: i64,
    pub correct: bool,
    pub points: i64,
    pub value: &'a str,
    pub ip: &'a str,
    pub at: i64,
}

/// Records one judged answer. A second correct answer for the same side, challenge and item
/// hits the unique index and is reported as a conflict, so simultaneous submissions and
/// retries can never score twice.
pub async fn insert_answer(pool: &SqlitePool, a: &NewAnswer<'_>) -> Result<(), ApiError> {
    let value: String = a.value.chars().take(200).collect();
    let done = sqlx::query("INSERT INTO duel_answer (tournament_id, match_id, challenge_id, item_id, side_team_id, user_id, verdict, points, value, ip, at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(a.tournament_id)
        .bind(a.match_id)
        .bind(a.challenge_id)
        .bind(a.item_id)
        .bind(a.side_team_id)
        .bind(a.user_id)
        .bind(if a.correct { "correct" } else { "incorrect" })
        .bind(a.points)
        .bind(value)
        .bind(a.ip)
        .bind(a.at)
        .execute(pool)
        .await;
    match done {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(ApiError::Conflict("already answered correctly".into()))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn match_state(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
) -> Result<Option<MatchState>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT opened_at, finalized_at FROM duel_match WHERE tournament_id = ? AND match_id = ?",
    )
    .bind(tournament_id)
    .bind(match_id)
    .fetch_optional(pool)
    .await?)
}

/// Starts the clock of a match, once.
pub async fn open_match(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO duel_match (tournament_id, match_id, opened_at) VALUES (?, ?, ?) ON CONFLICT(tournament_id, match_id) DO UPDATE SET opened_at = COALESCE(opened_at, excluded.opened_at)")
        .bind(tournament_id)
        .bind(match_id)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}

/// Marks the match finalized. True only for the caller that flipped it, so a result is written
/// once however many requests race to finish the match.
pub async fn claim_final(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
    now: i64,
) -> Result<bool, ApiError> {
    sqlx::query("INSERT OR IGNORE INTO duel_match (tournament_id, match_id) VALUES (?, ?)")
        .bind(tournament_id)
        .bind(match_id)
        .execute(pool)
        .await?;
    let done = sqlx::query("UPDATE duel_match SET finalized_at = ? WHERE tournament_id = ? AND match_id = ? AND finalized_at IS NULL")
        .bind(now)
        .bind(tournament_id)
        .bind(match_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn release_final(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE duel_match SET finalized_at = NULL WHERE tournament_id = ? AND match_id = ?",
    )
    .bind(tournament_id)
    .bind(match_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Matches that are open, have a time limit and are past it, for the background sweep.
pub async fn expired(pool: &SqlitePool, now: i64) -> Result<Vec<(i64, i64)>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT d.tournament_id, d.match_id FROM duel_match d JOIN tournament t ON t.id = d.tournament_id WHERE d.finalized_at IS NULL AND d.opened_at IS NOT NULL AND t.challenge_time_limit IS NOT NULL AND d.opened_at + t.challenge_time_limit <= ?",
    )
    .bind(now)
    .fetch_all(pool)
    .await?)
}

/// Forgets everything about one match, so it can be played again.
pub async fn clear_match(
    pool: &SqlitePool,
    tournament_id: i64,
    match_id: i64,
) -> Result<(), ApiError> {
    let mut tx = crate::db::begin_write(pool).await?;
    for table in ["duel_answer", "duel_match"] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE tournament_id = ? AND match_id = ?"
        ))
        .bind(tournament_id)
        .bind(match_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Forgets every assignment, answer and state of a tournament (used when it is reset).
pub async fn clear_tournament(pool: &SqlitePool, tournament_id: i64) -> Result<(), ApiError> {
    let mut tx = crate::db::begin_write(pool).await?;
    for table in ["duel_answer", "duel_match", "duel_assignment"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tournament_id = ?"))
            .bind(tournament_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Every question of every DFIR challenge of an event.
pub async fn items_of_event(pool: &SqlitePool, event_id: i64) -> Result<Vec<ItemRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT i.* FROM dfir_item i JOIN challenge c ON c.id = i.challenge_id WHERE c.event_id = ? ORDER BY i.challenge_id, i.position, i.id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?)
}
