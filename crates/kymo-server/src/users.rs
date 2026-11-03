use std::str::FromStr;

use kymo_core::Role;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;
use crate::text::is_hidden_char;

const MIN_USERNAME: usize = 3;
const MAX_USERNAME: usize = 32;
const MAX_EMAIL: usize = 254;
pub const MAX_DISPLAY_NAME: usize = 40;
pub const MAX_BIO: usize = 500;
const SETTING_REGISTRATION: &str = "registration_open";

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub role: String,
    pub banned: bool,
    pub display_name: String,
    pub bio: String,
    pub created_at: i64,
}

/// What the API shows about an account. Never carries the password hash.
#[derive(Debug, Clone, Serialize)]
pub struct UserOut {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub role: Role,
    pub banned: bool,
    pub display_name: String,
    pub bio: String,
    pub created_at: i64,
}

impl TryFrom<UserRow> for UserOut {
    type Error = ApiError;

    fn try_from(row: UserRow) -> Result<Self, ApiError> {
        Ok(UserOut {
            role: Role::from_str(&row.role).map_err(|_| ApiError::Internal)?,
            id: row.id,
            username: row.username,
            email: row.email,
            banned: row.banned,
            display_name: row.display_name,
            bio: row.bio,
            created_at: row.created_at,
        })
    }
}

pub fn check_username(raw: &str) -> Result<&str, ApiError> {
    let len = raw.chars().count();
    let ok = (MIN_USERNAME..=MAX_USERNAME).contains(&len)
        && !raw.chars().any(is_hidden_char)
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(raw)
    } else {
        Err(ApiError::BadRequest(format!(
            "the username is {MIN_USERNAME} to {MAX_USERNAME} characters: letters, digits, _ and -"
        )))
    }
}

/// Returns the lowercase email, or an error when it is clearly not one.
pub fn check_email(raw: &str) -> Result<String, ApiError> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= MAX_EMAIL
        && !email
            .chars()
            .any(|c| is_hidden_char(c) || c.is_whitespace())
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if valid {
        Ok(email)
    } else {
        Err(ApiError::BadRequest(
            "that email address is not valid".into(),
        ))
    }
}

pub fn check_display_name(raw: &str) -> Result<String, ApiError> {
    let name = raw.trim();
    if name.is_empty()
        || name.chars().count() > MAX_DISPLAY_NAME
        || name.chars().any(is_hidden_char)
    {
        return Err(ApiError::BadRequest(format!(
            "the display name is 1 to {MAX_DISPLAY_NAME} characters of plain text"
        )));
    }
    Ok(name.to_owned())
}

pub fn check_bio(raw: &str) -> Result<String, ApiError> {
    let bio = raw.trim();
    if bio.chars().count() > MAX_BIO || bio.chars().any(|c| is_hidden_char(c) && c != '\n') {
        return Err(ApiError::BadRequest(format!(
            "the bio is at most {MAX_BIO} characters of plain text"
        )));
    }
    Ok(bio.to_owned())
}

pub async fn registration_open(pool: &SqlitePool) -> Result<bool, ApiError> {
    let stored: Option<String> = sqlx::query_scalar("SELECT value FROM setting WHERE key = ?")
        .bind(SETTING_REGISTRATION)
        .fetch_optional(pool)
        .await?;
    Ok(stored.is_none_or(|json| json != "false"))
}

pub async fn set_registration_open(pool: &SqlitePool, open: bool) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(SETTING_REGISTRATION)
        .bind(if open { "true" } else { "false" })
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create(
    pool: &SqlitePool,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
    now: i64,
) -> Result<i64, ApiError> {
    let taken: Option<i64> = sqlx::query_scalar("SELECT id FROM user WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    if taken.is_some() {
        return Err(ApiError::Conflict("that username is taken".into()));
    }
    if let Some(email) = email {
        let taken: Option<i64> = sqlx::query_scalar("SELECT id FROM user WHERE email = ?")
            .bind(email)
            .fetch_optional(pool)
            .await?;
        if taken.is_some() {
            return Err(ApiError::Conflict(
                "that email is already registered".into(),
            ));
        }
    }
    let done = sqlx::query(
        "INSERT INTO user (username, email, password_hash, display_name, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(done.last_insert_rowid())
}

pub async fn by_username(pool: &SqlitePool, username: &str) -> Result<Option<UserRow>, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM user WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?)
}

pub async fn by_id(pool: &SqlitePool, id: i64) -> Result<UserRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM user WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub async fn update_profile(
    pool: &SqlitePool,
    id: i64,
    display_name: Option<&str>,
    bio: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE user SET display_name = COALESCE(?, display_name), bio = COALESCE(?, bio) WHERE id = ?",
    )
    .bind(display_name)
    .bind(bio)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_password(pool: &SqlitePool, id: i64, hash: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE user SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Serialize)]
pub struct UserPage {
    pub items: Vec<UserOut>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

/// Escapes LIKE wildcards so a search for `%` finds a literal percent sign.
fn like_pattern(query: &str) -> String {
    let mut out = String::from("%");
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

pub async fn list(
    pool: &SqlitePool,
    query: Option<&str>,
    page: i64,
    per_page: i64,
) -> Result<UserPage, ApiError> {
    let pattern = like_pattern(query.unwrap_or(""));
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user WHERE username LIKE ?1 ESCAPE '\\' OR COALESCE(email, '') LIKE ?1 ESCAPE '\\'",
    )
    .bind(&pattern)
    .fetch_one(pool)
    .await?;
    let rows: Vec<UserRow> = sqlx::query_as(
        "SELECT * FROM user WHERE username LIKE ?1 ESCAPE '\\' OR COALESCE(email, '') LIKE ?1 ESCAPE '\\' ORDER BY id LIMIT ?2 OFFSET ?3",
    )
    .bind(&pattern)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(pool)
    .await?;
    Ok(UserPage {
        items: rows
            .into_iter()
            .map(UserOut::try_from)
            .collect::<Result<_, _>>()?,
        total,
        page,
        per_page,
    })
}

/// Changes the role and/or the ban flag. A ban also ends the user's sessions.
pub async fn moderate(
    pool: &SqlitePool,
    id: i64,
    role: Option<Role>,
    banned: Option<bool>,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await?;
    let done = sqlx::query(
        "UPDATE user SET role = COALESCE(?, role), banned = COALESCE(?, banned) WHERE id = ?",
    )
    .bind(role.map(Role::as_str))
    .bind(banned)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    if banned == Some(true) {
        sqlx::query("DELETE FROM token WHERE user_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
