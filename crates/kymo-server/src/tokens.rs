use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;

pub const SESSION_SECONDS: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Session,
    Api,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Session => "session",
            Kind::Api => "api",
        }
    }
}

#[derive(Debug, Serialize, FromRow)]
pub struct ApiToken {
    pub id: i64,
    pub label: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    /// A referee token can only record results and schedules, for the listed tournaments.
    pub referee: bool,
    #[sqlx(json(nullable))]
    pub scope: Option<Vec<i64>>,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn fresh_secret() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Only the hash is stored, so a leaked database does not leak usable tokens.
pub fn hash(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Creates a token and returns its secret, which is shown to the caller exactly once.
/// `api_days` limits an API token's life; sessions always use `SESSION_SECONDS`.
pub async fn issue(
    pool: &SqlitePool,
    kind: Kind,
    label: &str,
    api_days: Option<u32>,
) -> Result<String, ApiError> {
    let secret = fresh_secret();
    let created = now();
    let expires = match kind {
        Kind::Session => Some(created + SESSION_SECONDS),
        Kind::Api => api_days.map(|d| created + i64::from(d) * 24 * 3600),
    };
    sqlx::query(
        "INSERT INTO token (hash, kind, label, created_at, expires_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash(&secret))
    .bind(kind.as_str())
    .bind(label)
    .bind(created)
    .bind(expires)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM token WHERE expires_at IS NOT NULL AND expires_at < ?")
        .bind(created)
        .execute(pool)
        .await?;
    Ok(secret)
}

/// What a valid token may do. `scope` is only meaningful for referees.
pub struct Access {
    pub referee: bool,
    pub scope: Vec<i64>,
}

pub async fn access(pool: &SqlitePool, secret: &str) -> Result<Option<Access>, ApiError> {
    let row: Option<(bool, Option<String>)> = sqlx::query_as(
        "SELECT referee, scope FROM token WHERE hash = ? AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(hash(secret))
    .bind(now())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(referee, scope)| Access {
        referee,
        scope: scope
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default(),
    }))
}

/// Creates an API token limited to recording results and schedules for `tournaments`.
pub async fn issue_referee(
    pool: &SqlitePool,
    label: &str,
    api_days: Option<u32>,
    tournaments: &[i64],
) -> Result<String, ApiError> {
    let secret = issue(pool, Kind::Api, label, api_days).await?;
    let scope = serde_json::to_string(tournaments).map_err(|_| ApiError::Internal)?;
    sqlx::query("UPDATE token SET referee = 1, scope = ? WHERE hash = ?")
        .bind(scope)
        .bind(hash(&secret))
        .execute(pool)
        .await?;
    Ok(secret)
}

pub async fn is_valid(pool: &SqlitePool, secret: &str) -> Result<bool, ApiError> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM token WHERE hash = ? AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(hash(secret))
    .bind(now())
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

/// True when the secret is a live organizer session, as opposed to an API token.
pub async fn is_session(pool: &SqlitePool, secret: &str) -> Result<bool, ApiError> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM token WHERE hash = ? AND kind = 'session' AND expires_at > ?",
    )
    .bind(hash(secret))
    .bind(now())
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

pub async fn list_api(pool: &SqlitePool) -> Result<Vec<ApiToken>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, label, created_at, expires_at, referee, scope FROM token WHERE kind = 'api' AND (expires_at IS NULL OR expires_at > ?) ORDER BY id DESC",
    )
    .bind(now())
    .fetch_all(pool)
    .await?)
}

pub async fn revoke_api(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let done = sqlx::query("DELETE FROM token WHERE id = ? AND kind = 'api'")
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

/// Deletes one session token. API tokens are left alone, they are revoked by id.
pub async fn end_session(pool: &SqlitePool, secret: &str) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM token WHERE hash = ? AND kind = 'session'")
        .bind(hash(secret))
        .execute(pool)
        .await?;
    Ok(())
}
