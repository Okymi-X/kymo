use kymo_engine::{Bracket, Side, Slot};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;
use crate::tokens::now;

const LIST_LIMIT: i64 = 200;

/// One line of a tournament's public history. `data` is a snapshot of the match after the
/// change (teams, score, winner, forfeit) so the log stays readable if the bracket later moves on.
#[derive(Debug, Serialize, FromRow)]
pub struct Event {
    pub id: i64,
    pub at: i64,
    pub kind: String,
    pub match_id: Option<i64>,
    #[sqlx(json)]
    pub data: Value,
}

async fn insert(
    pool: &SqlitePool,
    tournament_id: i64,
    at: i64,
    kind: &str,
    match_id: Option<usize>,
    data: Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO event (tournament_id, at, kind, match_id, data) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(tournament_id)
    .bind(at)
    .bind(kind)
    .bind(match_id.map(|m| m as i64))
    .bind(data.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_tournament(
    pool: &SqlitePool,
    tournament_id: i64,
    kind: &str,
    data: Value,
) -> Result<(), ApiError> {
    insert(pool, tournament_id, now(), kind, None, data.clone()).await?;
    crate::webhooks::dispatch(pool.clone(), tournament_id, kind.to_owned(), data);
    Ok(())
}

/// Records a change to one match using its state after the change.
pub async fn record_match(
    pool: &SqlitePool,
    tournament_id: i64,
    kind: &str,
    bracket: &Bracket,
    match_id: usize,
) -> Result<(), ApiError> {
    record_match_at(pool, tournament_id, kind, bracket, match_id, now()).await?;
    if let Some(data) = match_data(bracket, match_id) {
        crate::webhooks::dispatch(pool.clone(), tournament_id, kind.to_owned(), data);
    }
    Ok(())
}

fn match_data(bracket: &Bracket, match_id: usize) -> Option<Value> {
    let m = bracket.matches.get(match_id)?;
    let team = |slot: Slot| match slot {
        Slot::Team(id) => Some(id),
        _ => None,
    };
    Some(json!({
        "team_a": team(m.slots[0]),
        "team_b": team(m.slots[1]),
        "score_a": m.score.map(|s| s[0]),
        "score_b": m.score.map(|s| s[1]),
        "winner": m.winner.map(|s| if s == Side::A { "A" } else { "B" }),
        "forfeit": m.forfeit,
        "games": m.games,
        "scheduled_at": m.scheduled_at,
        "venue": m.venue,
    }))
}

/// Same as `record_match` with an explicit time, used to lay out demo data over a few hours.
pub async fn record_match_at(
    pool: &SqlitePool,
    tournament_id: i64,
    kind: &str,
    bracket: &Bracket,
    match_id: usize,
    at: i64,
) -> Result<(), ApiError> {
    let Some(data) = match_data(bracket, match_id) else {
        return Ok(());
    };
    insert(pool, tournament_id, at, kind, Some(match_id), data).await
}

pub async fn record_tournament_at(
    pool: &SqlitePool,
    tournament_id: i64,
    kind: &str,
    data: Value,
    at: i64,
) -> Result<(), ApiError> {
    insert(pool, tournament_id, at, kind, None, data).await
}

/// Newest first.
pub async fn list(pool: &SqlitePool, tournament_id: i64) -> Result<Vec<Event>, ApiError> {
    Ok(sqlx::query_as("SELECT id, at, kind, match_id, data FROM event WHERE tournament_id = ? ORDER BY id DESC LIMIT ?")
        .bind(tournament_id)
        .bind(LIST_LIMIT)
        .fetch_all(pool)
        .await?)
}
