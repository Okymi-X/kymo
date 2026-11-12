//! Storage for dynamic and multi flag challenges: the flags a participant's runs were given,
//! the flags a participant found, and who a flag was generated for. Rules about generating and
//! splitting live in kymo-jeopardy.

use std::collections::HashMap;

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::ApiError;
use crate::jeopardy_store::{NewSubmission, Scorer};

/// Highest id offset for the board's flag pseudo challenges, apart from DFIR questions.
pub const FLAG_BASE: u64 = 2 << 40;

/// The generated flag values of this participant's runs of a challenge, by challenge flag.
/// A run that never started does not count; one that ran, then ended or failed, still does,
/// because its flag may already have been found.
pub async fn run_values(
    pool: &SqlitePool,
    challenge_id: i64,
    scorer: Scorer,
) -> Result<HashMap<i64, Vec<String>>, ApiError> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT f.flag_id, f.value FROM instance_flag f JOIN instance i ON i.id = f.instance_id WHERE i.challenge_id = ? AND i.scorer_kind = ? AND i.scorer_id = ? AND (i.failure IS NULL OR i.failure <> 'start_failed')",
    )
    .bind(challenge_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .fetch_all(pool)
    .await?;
    let mut by_flag: HashMap<i64, Vec<String>> = HashMap::new();
    for (flag_id, value) in rows {
        by_flag.entry(flag_id).or_default().push(value);
    }
    Ok(by_flag)
}

/// The flags this participant has found on a challenge, with the points each earned.
pub async fn found(
    pool: &SqlitePool,
    challenge_id: i64,
    scorer: Scorer,
) -> Result<HashMap<i64, i64>, ApiError> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT flag_id, points FROM flag_solve WHERE challenge_id = ? AND participant_id = ?",
    )
    .bind(challenge_id)
    .bind(scorer.id())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Flags found per challenge by one participant, for the challenge list.
pub async fn found_counts(
    pool: &SqlitePool,
    event_id: i64,
    scorer: Scorer,
) -> Result<HashMap<i64, u32>, ApiError> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT challenge_id, COUNT(*) FROM flag_solve WHERE event_id = ? AND participant_id = ? GROUP BY challenge_id",
    )
    .bind(event_id)
    .bind(scorer.id())
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, n)| (id, u32::try_from(n).unwrap_or(u32::MAX)))
        .collect())
}

/// Whom a generated flag value was made for, if it was made for anyone on this challenge.
pub async fn owner_of_value(
    pool: &SqlitePool,
    challenge_id: i64,
    value: &str,
) -> Result<Option<(String, i64)>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT i.scorer_kind, i.scorer_id FROM instance_flag f JOIN instance i ON i.id = f.instance_id WHERE i.challenge_id = ? AND f.value = ? LIMIT 1",
    )
    .bind(challenge_id)
    .bind(value)
    .fetch_optional(pool)
    .await?)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_flag_solve(
    tx: &mut Transaction<'_, Sqlite>,
    s: &NewSubmission<'_>,
    flag_id: i64,
    points: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO flag_solve (event_id, challenge_id, flag_id, participant_id, user_id, points, at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.event_id)
    .bind(s.challenge_id)
    .bind(flag_id)
    .bind(s.scorer.id())
    .bind(s.user_id)
    .bind(points)
    .bind(s.at)
    .execute(&mut **tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("you already have this flag".into())
        }
        _ => e.into(),
    })?;
    Ok(())
}
