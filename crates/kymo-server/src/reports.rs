use kymo_engine::{Slot, TeamId};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::db;
use crate::error::ApiError;
use crate::tokens::now;

const MAX_SCORE: u32 = 1_000_000;

fn fresh_code() -> String {
    rand::random::<[u8; 10]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Creates or replaces a team's private reporting code. Returns the new code.
pub async fn issue_code(
    pool: &SqlitePool,
    tournament_id: i64,
    team_id: i64,
) -> Result<String, ApiError> {
    let code = fresh_code();
    let done = sqlx::query("UPDATE team SET code = ? WHERE id = ? AND tournament_id = ?")
        .bind(&code)
        .bind(team_id)
        .bind(tournament_id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(code)
}

/// What a captain sees: their team and the match they are expected to report.
#[derive(Debug, Serialize)]
pub struct Assignment {
    pub tournament: String,
    pub slug: String,
    pub team: String,
    pub match_id: Option<usize>,
    pub opponent: Option<String>,
    pub round: Option<u32>,
    pub reported: Option<[u32; 2]>,
}

struct Who {
    team_id: i64,
    tournament_id: i64,
}

async fn who(pool: &SqlitePool, code: &str) -> Result<Who, ApiError> {
    let row: Option<(i64, i64)> =
        sqlx::query_as("SELECT id, tournament_id FROM team WHERE code = ?")
            .bind(code)
            .fetch_optional(pool)
            .await?;
    row.map(|(team_id, tournament_id)| Who {
        team_id,
        tournament_id,
    })
    .ok_or(ApiError::NotFound)
}

/// The team's next ready, unplayed match and the sides of it.
fn next_match(t: &db::Tournament, team: i64) -> Option<(usize, bool, TeamId, u32)> {
    let team = TeamId::try_from(team).ok()?;
    t.bracket.as_ref()?.matches.iter().find_map(|m| {
        if !m.is_ready() || m.is_played() {
            return None;
        }
        match m.slots {
            [Slot::Team(a), Slot::Team(b)] if a == team => Some((m.id, true, b, m.round)),
            [Slot::Team(a), Slot::Team(b)] if b == team => Some((m.id, false, a, m.round)),
            _ => None,
        }
    })
}

pub async fn assignment(pool: &SqlitePool, code: &str) -> Result<Assignment, ApiError> {
    let w = who(pool, code).await?;
    let t = db::get(pool, w.tournament_id).await?;
    let team = t
        .teams
        .iter()
        .find(|x| x.id == w.team_id)
        .map(|x| x.name.clone())
        .unwrap_or_default();
    let next = next_match(&t, w.team_id);
    let name_of = |id: TeamId| {
        t.teams
            .iter()
            .find(|x| x.id as u32 == id)
            .map(|x| x.name.clone())
    };
    let reported = match next {
        Some((match_id, own_is_a, _, _)) => {
            let row: Option<(i64, i64)> = sqlx::query_as(
                "SELECT score_a, score_b FROM report WHERE tournament_id = ? AND match_id = ? AND team_id = ?",
            )
            .bind(w.tournament_id)
            .bind(match_id as i64)
            .bind(w.team_id)
            .fetch_optional(pool)
            .await?;
            row.map(|(a, b)| {
                if own_is_a {
                    [a as u32, b as u32]
                } else {
                    [b as u32, a as u32]
                }
            })
        }
        None => None,
    };
    Ok(Assignment {
        tournament: t.name.clone(),
        slug: t.slug.clone(),
        team,
        match_id: next.map(|n| n.0),
        opponent: next.and_then(|n| name_of(n.2)),
        round: next.map(|n| n.3),
        reported,
    })
}

/// Stores the captain's score as a pending report. A later report from the same team replaces it.
/// `own` is the reporting team's score and `other` the opponent's.
pub async fn submit(pool: &SqlitePool, code: &str, own: u32, other: u32) -> Result<(), ApiError> {
    if own > MAX_SCORE || other > MAX_SCORE {
        return Err(ApiError::BadRequest(format!(
            "scores must be at most {MAX_SCORE}"
        )));
    }
    let w = who(pool, code).await?;
    let t = db::get(pool, w.tournament_id).await?;
    let (match_id, own_is_a, _, _) = next_match(&t, w.team_id)
        .ok_or_else(|| ApiError::Conflict("there is no match to report right now".into()))?;
    let (a, b) = if own_is_a { (own, other) } else { (other, own) };
    sqlx::query(
        "INSERT INTO report (tournament_id, match_id, team_id, score_a, score_b, created_at) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(tournament_id, match_id, team_id) DO UPDATE SET score_a = excluded.score_a, score_b = excluded.score_b, created_at = excluded.created_at",
    )
    .bind(w.tournament_id)
    .bind(match_id as i64)
    .bind(w.team_id)
    .bind(i64::from(a))
    .bind(i64::from(b))
    .bind(now())
    .execute(pool)
    .await?;
    Ok(())
}

/// A report waiting for the organizer, with names, and whether both teams agree.
#[derive(Debug, Serialize, FromRow)]
pub struct Pending {
    pub id: i64,
    pub tournament_id: i64,
    pub tournament: String,
    pub match_id: i64,
    pub team: String,
    pub score_a: i64,
    pub score_b: i64,
    pub created_at: i64,
}

pub async fn pending(pool: &SqlitePool) -> Result<Vec<Pending>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT r.id, r.tournament_id, t.name AS tournament, r.match_id, team.name AS team, r.score_a, r.score_b, r.created_at
         FROM report r JOIN tournament t ON t.id = r.tournament_id JOIN team ON team.id = r.team_id
         ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?)
}

pub struct Report {
    pub tournament_id: i64,
    pub match_id: usize,
    pub score_a: u32,
    pub score_b: u32,
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Report, ApiError> {
    let row: Option<(i64, i64, i64, i64)> =
        sqlx::query_as("SELECT tournament_id, match_id, score_a, score_b FROM report WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    row.map(|(tournament_id, match_id, a, b)| Report {
        tournament_id,
        match_id: match_id as usize,
        score_a: a as u32,
        score_b: b as u32,
    })
    .ok_or(ApiError::NotFound)
}

/// Removes a report and every other report for the same match, once it is decided either way.
pub async fn close(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let r = get(pool, id).await?;
    sqlx::query("DELETE FROM report WHERE tournament_id = ? AND match_id = ?")
        .bind(r.tournament_id)
        .bind(r.match_id as i64)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn reject(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let done = sqlx::query("DELETE FROM report WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}
