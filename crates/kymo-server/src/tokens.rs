use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use kymo_core::Role;
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
    insert(pool, kind, label, api_days, None).await
}

async fn insert(
    pool: &SqlitePool,
    kind: Kind,
    label: &str,
    api_days: Option<u32>,
    user_id: Option<i64>,
) -> Result<String, ApiError> {
    let secret = fresh_secret();
    let created = now();
    let expires = match kind {
        Kind::Session => Some(created + SESSION_SECONDS),
        Kind::Api => api_days.map(|d| created + i64::from(d) * 24 * 3600),
    };
    sqlx::query(
        "INSERT INTO token (hash, kind, label, created_at, expires_at, user_id) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(hash(&secret))
    .bind(kind.as_str())
    .bind(label)
    .bind(created)
    .bind(expires)
    .bind(user_id)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM token WHERE expires_at IS NOT NULL AND expires_at < ?")
        .bind(created)
        .execute(pool)
        .await?;
    Ok(secret)
}

/// The account behind a user session.
#[derive(Debug, Clone, Copy)]
pub struct TokenUser {
    pub id: i64,
    pub role: Role,
}

/// What a valid token may do. `scope` is only meaningful for referees.
pub struct Access {
    pub referee: bool,
    pub scope: Vec<i64>,
    /// True for session tokens, false for API tokens.
    pub session: bool,
    /// Set for player sessions. Legacy organizer sessions and API tokens have no account.
    pub user: Option<TokenUser>,
}

impl Access {
    /// The role this token acts with. The legacy organizer password and non referee API
    /// tokens act as admin; a user session acts with the role stored on the account.
    pub fn role(&self) -> Role {
        match (&self.user, self.referee) {
            (Some(user), _) => user.role,
            (None, true) => Role::Referee,
            (None, false) => Role::Admin,
        }
    }
}

type AccessRow = (
    String,
    bool,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<bool>,
);

/// Looks up a live token. A token of a banned account is treated as invalid.
pub async fn access(pool: &SqlitePool, secret: &str) -> Result<Option<Access>, ApiError> {
    let row: Option<AccessRow> = sqlx::query_as(
        "SELECT t.kind, t.referee, t.scope, t.user_id, u.role, u.banned FROM token t LEFT JOIN user u ON u.id = t.user_id WHERE t.hash = ? AND (t.expires_at IS NULL OR t.expires_at > ?)",
    )
    .bind(hash(secret))
    .bind(now())
    .fetch_optional(pool)
    .await?;
    let Some((kind, referee, scope, user_id, role, banned)) = row else {
        return Ok(None);
    };
    let user = match (user_id, role) {
        (Some(id), Some(role)) => {
            if banned.unwrap_or(false) {
                return Ok(None);
            }
            let role = Role::from_str(&role).map_err(|_| ApiError::Internal)?;
            Some(TokenUser { id, role })
        }
        (Some(_), None) => return Ok(None),
        (None, _) => None,
    };
    Ok(Some(Access {
        referee,
        session: kind == "session",
        user,
        scope: scope
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default(),
    }))
}

/// Creates a session for a player account, with the same lifetime as an organizer session.
pub async fn issue_user(pool: &SqlitePool, user_id: i64) -> Result<String, ApiError> {
    insert(pool, Kind::Session, "", None, Some(user_id)).await
}

/// Ends every session of a user except the one named by `keep` (a secret), if any.
pub async fn end_user_sessions(
    pool: &SqlitePool,
    user_id: i64,
    keep: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM token WHERE user_id = ? AND hash <> ?")
        .bind(user_id)
        .bind(keep.map(hash).unwrap_or_default())
        .execute(pool)
        .await?;
    Ok(())
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
        "SELECT id FROM token WHERE hash = ? AND kind = 'session' AND user_id IS NULL AND expires_at > ?",
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
