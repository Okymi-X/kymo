use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;
use crate::tokens::now;

pub const MAX_HOOKS: i64 = 10;
const MAX_URL: usize = 500;
const SEND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, FromRow)]
struct Hook {
    id: i64,
    label: String,
    url: String,
}

/// What the API shows: the host only, because chat webhook URLs carry a secret in the path.
#[derive(Debug, Serialize)]
pub struct HookView {
    pub id: i64,
    pub label: String,
    pub host: String,
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or("")
        .to_owned()
}

fn view(hook: Hook) -> HookView {
    HookView {
        id: hook.id,
        host: host_of(&hook.url),
        label: hook.label,
    }
}

/// Only plain http and https URLs without whitespace or control characters.
pub fn validate_url(url: &str) -> Result<(), ApiError> {
    let ok = (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= MAX_URL
        && !host_of(url).is_empty()
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if ok {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "webhook URL must start with http:// or https://".into(),
        ))
    }
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<HookView>, ApiError> {
    let hooks: Vec<Hook> = sqlx::query_as("SELECT id, label, url FROM webhook ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(hooks.into_iter().map(view).collect())
}

pub async fn add(pool: &SqlitePool, label: &str, url: &str) -> Result<HookView, ApiError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webhook")
        .fetch_one(pool)
        .await?;
    if count >= MAX_HOOKS {
        return Err(ApiError::Conflict(format!("at most {MAX_HOOKS} webhooks")));
    }
    let id = sqlx::query("INSERT INTO webhook (label, url, created_at) VALUES (?, ?, ?)")
        .bind(label)
        .bind(url)
        .bind(now())
        .execute(pool)
        .await?
        .last_insert_rowid();
    Ok(HookView {
        id,
        label: label.to_owned(),
        host: host_of(url),
    })
}

pub async fn remove(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let done = sqlx::query("DELETE FROM webhook WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(SEND_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()
}

fn payload(message: &str, kind: &str, tournament: &str, slug: &str) -> Value {
    // `content` is read by Discord and `text` by Slack; other receivers get the structured fields.
    json!({ "content": message, "text": message, "event": kind, "tournament": tournament, "slug": slug })
}

/// Posts one message to one hook and reports the status, for the Test button.
pub async fn send_test(pool: &SqlitePool, id: i64) -> Result<u16, ApiError> {
    let hook: Hook = sqlx::query_as("SELECT id, label, url FROM webhook WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let client = client().ok_or(ApiError::Internal)?;
    let body = payload("Kymo test message: this webhook works.", "test", "Kymo", "");
    match client.post(&hook.url).json(&body).send().await {
        Ok(response) => Ok(response.status().as_u16()),
        Err(_) => Err(ApiError::BadRequest(
            "the webhook could not be reached".into(),
        )),
    }
}

async fn team_name(pool: &SqlitePool, id: Option<i64>) -> String {
    let Some(id) = id else {
        return "TBD".into();
    };
    sqlx::query_scalar("SELECT name FROM team WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "TBD".into())
}

/// `HH:MM UTC` for a unix time, enough for a chat line.
fn clock(at: i64) -> String {
    let secs = at.rem_euclid(86_400);
    format!("{:02}:{:02} UTC", secs / 3600, secs % 3600 / 60)
}

/// The sentence a chat shows for one log event.
pub async fn describe(
    pool: &SqlitePool,
    tournament: &str,
    kind: &str,
    data: &Value,
) -> Option<String> {
    let id = |key: &str| data.get(key).and_then(Value::as_i64);
    let a = team_name(pool, id("team_a")).await;
    let b = team_name(pool, id("team_b")).await;
    let winner_is_a = data.get("winner").and_then(Value::as_str) == Some("A");
    let (winner, loser) = if winner_is_a { (&a, &b) } else { (&b, &a) };
    let score = match (id("score_a"), id("score_b")) {
        (Some(x), Some(y)) => format!(" {x}:{y}"),
        _ => String::new(),
    };
    let has_winner = data.get("winner").is_some_and(|w| !w.is_null());
    Some(match kind {
        "started" => format!(
            "{tournament} started with {} teams",
            id("teams").unwrap_or(0)
        ),
        "reset" => format!("{tournament} was reset by the organizer"),
        "result" | "corrected" => {
            let prefix = if kind == "corrected" {
                "Corrected: "
            } else {
                ""
            };
            if has_winner {
                format!("{tournament}: {prefix}{winner} beat {loser}{score}")
            } else {
                format!("{tournament}: {prefix}{a} and {b} drew{score}")
            }
        }
        "forfeit" => format!("{tournament}: {winner} advanced by forfeit against {loser}"),
        "live" => format!("{tournament}: {a} versus {b} is starting now"),
        "cleared" => format!("{tournament}: result cleared for {a} versus {b}"),
        "scheduled" => match id("scheduled_at") {
            Some(at) => {
                let venue = data
                    .get("venue")
                    .and_then(Value::as_str)
                    .map(|v| format!(" in {v}"))
                    .unwrap_or_default();
                format!("{tournament}: {a} versus {b} at {}{venue}", clock(at))
            }
            None => format!("{tournament}: schedule removed for {a} versus {b}"),
        },
        _ => return None,
    })
}

/// Sends a log event to every webhook without making the request wait for it. A hook that is
/// slow, down or wrong never affects the organizer's action.
pub fn dispatch(pool: SqlitePool, tournament_id: i64, kind: String, data: Value) {
    tokio::spawn(async move {
        let hooks: Vec<Hook> = match sqlx::query_as("SELECT id, label, url FROM webhook")
            .fetch_all(&pool)
            .await
        {
            Ok(hooks) if !hooks.is_empty() => hooks,
            _ => return,
        };
        let Ok((name, slug)) =
            sqlx::query_as::<_, (String, String)>("SELECT name, slug FROM tournament WHERE id = ?")
                .bind(tournament_id)
                .fetch_one(&pool)
                .await
        else {
            return;
        };
        let Some(message) = describe(&pool, &name, &kind, &data).await else {
            return;
        };
        let Some(client) = client() else {
            return;
        };
        let body = payload(&message, &kind, &name, &slug);
        for hook in hooks {
            if let Err(e) = client.post(&hook.url).json(&body).send().await {
                tracing::warn!(hook = hook.id, label = %hook.label, error = %e, "webhook delivery failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_checked_and_only_the_host_is_shown() {
        assert!(validate_url("https://discord.com/api/webhooks/1/abc").is_ok());
        assert!(validate_url("http://localhost:9000/hook").is_ok());
        for bad in [
            "ftp://x.com/a",
            "https://",
            "javascript:alert(1)",
            "https://a b.com",
            "",
        ] {
            assert!(validate_url(bad).is_err(), "{bad}");
        }
        assert_eq!(
            host_of("https://discord.com/api/webhooks/1/secret"),
            "discord.com"
        );
        assert_eq!(host_of("http://localhost:9000/x?y=1"), "localhost:9000");
    }

    #[test]
    fn clock_formats_utc_time() {
        assert_eq!(clock(3 * 3600 + 5 * 60), "03:05 UTC");
        assert_eq!(clock(86_400 + 18 * 3600 + 40 * 60), "18:40 UTC");
    }
}
