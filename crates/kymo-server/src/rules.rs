//! The rules of an event: a title and a text, shown to players in a dialog on the event page.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::ApiError;
use crate::text::is_hidden_char;

const MAX_TITLE: usize = 60;
const MAX_BODY: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub title: String,
    pub body: String,
}

impl Rules {
    pub fn none() -> Self {
        Self {
            title: "Rules".into(),
            body: String::new(),
        }
    }

    /// Trims the text and refuses what is too long or hides characters. An empty title falls
    /// back to "Rules".
    pub fn validate(&mut self) -> Result<(), ApiError> {
        self.title = self.title.trim().to_owned();
        if self.title.is_empty() {
            self.title = "Rules".into();
        }
        if self.title.chars().count() > MAX_TITLE || self.title.chars().any(is_hidden_char) {
            return Err(ApiError::BadRequest(format!(
                "the title is at most {MAX_TITLE} characters, on one line"
            )));
        }
        self.body = self.body.replace("\r\n", "\n").trim().to_owned();
        let hidden = self
            .body
            .chars()
            .any(|c| is_hidden_char(c) && c != '\n' && c != '\t');
        if self.body.chars().count() > MAX_BODY || hidden {
            return Err(ApiError::BadRequest(format!(
                "the rules are at most {MAX_BODY} characters"
            )));
        }
        Ok(())
    }
}

pub async fn get(pool: &SqlitePool, event_id: i64) -> Result<Rules, ApiError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT title, body FROM event_rules WHERE event_id = ?")
            .bind(event_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map_or_else(Rules::none, |(title, body)| Rules { title, body }))
}

/// Saves the rules; an empty text removes them.
pub async fn put(
    pool: &SqlitePool,
    event_id: i64,
    rules: &Rules,
    now: i64,
) -> Result<(), ApiError> {
    if rules.body.is_empty() {
        sqlx::query("DELETE FROM event_rules WHERE event_id = ?")
            .bind(event_id)
            .execute(pool)
            .await?;
        return Ok(());
    }
    sqlx::query("INSERT INTO event_rules (event_id, title, body, updated_at) VALUES (?, ?, ?, ?) ON CONFLICT(event_id) DO UPDATE SET title = excluded.title, body = excluded.body, updated_at = excluded.updated_at")
        .bind(event_id)
        .bind(&rules.title)
        .bind(&rules.body)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}
