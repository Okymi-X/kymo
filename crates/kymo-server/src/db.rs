use std::str::FromStr;

use kymo_engine::{Bracket, Format};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;

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
}

#[derive(Debug, Clone, FromRow)]
struct Row {
    id: i64,
    name: String,
    slug: String,
    format: String,
    third_place: bool,
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
    pub status: Status,
    pub created_at: String,
    pub teams: Vec<Team>,
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

fn format_to_text(format: Format) -> &'static str {
    match format {
        Format::SingleElimination => "single_elimination",
        Format::DoubleElimination => "double_elimination",
        Format::RoundRobin => "round_robin",
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
        status: row.status,
        created_at: row.created_at,
        teams,
    })
}

async fn teams_of(pool: &SqlitePool, id: i64) -> Result<Vec<Team>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, name, seed, score FROM team WHERE tournament_id = ? ORDER BY seed",
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Tournament>, ApiError> {
    let rows: Vec<Row> = sqlx::query_as("SELECT * FROM tournament ORDER BY id DESC")
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
    let row: Row = sqlx::query_as("SELECT * FROM tournament WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    let teams = teams_of(pool, row.id).await?;
    hydrate(row, teams)
}

pub async fn get_by_slug(pool: &SqlitePool, slug: &str) -> Result<Tournament, ApiError> {
    let row: Row = sqlx::query_as("SELECT * FROM tournament WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await?;
    let teams = teams_of(pool, row.id).await?;
    hydrate(row, teams)
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    slug: &str,
    format: Format,
    third_place: bool,
) -> Result<i64, ApiError> {
    let result =
        sqlx::query("INSERT INTO tournament (name, slug, format, third_place) VALUES (?, ?, ?, ?)")
            .bind(name)
            .bind(slug)
            .bind(format_to_text(format))
            .bind(third_place)
            .execute(pool)
            .await?;
    Ok(result.last_insert_rowid())
}

pub async fn rename(pool: &SqlitePool, id: i64, name: &str) -> Result<(), ApiError> {
    let done = sqlx::query("UPDATE tournament SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let done = sqlx::query("DELETE FROM tournament WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

pub struct NewTeam {
    pub name: String,
    pub score: i64,
}

/// Replaces the whole roster; list order defines the seeds.
pub async fn replace_teams(pool: &SqlitePool, id: i64, teams: &[NewTeam]) -> Result<(), ApiError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM team WHERE tournament_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (index, team) in teams.iter().enumerate() {
        sqlx::query("INSERT INTO team (tournament_id, name, seed, score) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(&team.name)
            .bind(index as i64 + 1)
            .bind(team.score)
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
    Ok(())
}
