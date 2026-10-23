use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;

const KEY: &str = "appearance";
const MAX_SITE_NAME: usize = 40;
const MAX_RADIUS: u8 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontChoice {
    Sans,
    Serif,
    Mono,
}

/// How the instance looks. Stored as one JSON row so new options need no migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub site_name: String,
    pub accent_light: String,
    pub accent_dark: String,
    pub radius: u8,
    pub font: FontChoice,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            site_name: "Kymo".into(),
            accent_light: "#0a6fe0".into(),
            accent_dark: "#4da3ff".into(),
            radius: 8,
            font: FontChoice::Sans,
        }
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl Appearance {
    pub fn validate(&mut self) -> Result<(), ApiError> {
        self.site_name = self.site_name.trim().to_owned();
        let name_len = self.site_name.chars().count();
        if name_len == 0 || name_len > MAX_SITE_NAME {
            return Err(ApiError::BadRequest(format!(
                "site name must be 1 to {MAX_SITE_NAME} characters"
            )));
        }
        if !is_hex_color(&self.accent_light) || !is_hex_color(&self.accent_dark) {
            return Err(ApiError::BadRequest("colors must look like #0a6fe0".into()));
        }
        if self.radius > MAX_RADIUS {
            return Err(ApiError::BadRequest(format!(
                "radius must be at most {MAX_RADIUS}"
            )));
        }
        Ok(())
    }
}

pub async fn load(pool: &SqlitePool) -> Result<Appearance, ApiError> {
    let stored: Option<String> = sqlx::query_scalar("SELECT value FROM setting WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await?;
    Ok(stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

pub async fn save(pool: &SqlitePool, appearance: &Appearance) -> Result<(), ApiError> {
    let json = serde_json::to_string(appearance).map_err(|_| ApiError::Internal)?;
    sqlx::query("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(KEY)
        .bind(json)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn reset(pool: &SqlitePool) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM setting WHERE key = ?")
        .bind(KEY)
        .execute(pool)
        .await?;
    Ok(())
}
