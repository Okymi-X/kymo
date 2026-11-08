//! Storage and validation of Jeopardy categories, challenges, hints and attachments.
//! Rules about scoring and flags live in kymo-jeopardy; this module only checks and stores.

use kymo_jeopardy::{FlagSpec, Scoring};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};

use crate::error::ApiError;

pub const MAX_ATTACHMENT: usize = 10 * 1024 * 1024;
pub const MAX_TITLE: usize = 120;
const MAX_CHALLENGE_DESCRIPTION: usize = 20_000;
const MAX_HINT: usize = 5_000;
const MAX_POINTS: i64 = 1_000_000;
const MAX_FORMAT: usize = 100;
const MAX_FLAG: usize = 500;
const MAX_INSTANCE_CONFIG: usize = 16 * 1024;
const MAX_FILENAME: usize = 100;
const EARLIEST: i64 = 946_684_800;
const LATEST: i64 = 4_102_444_800;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CategoryRow {
    pub id: i64,
    pub event_id: i64,
    pub name: String,
    pub position: i64,
}

/// A stored challenge. Deliberately not `Serialize`: it carries the flag, so every output goes
/// through an explicit struct.
#[derive(Debug, Clone, FromRow)]
pub struct ChallengeRow {
    pub id: i64,
    pub event_id: i64,
    pub category_id: i64,
    pub title: String,
    pub description: String,
    #[sqlx(json)]
    pub scoring: Scoring,
    #[sqlx(json)]
    pub flag: FlagSpec,
    pub flag_prefix: String,
    pub flag_suffix: String,
    pub case_sensitive: bool,
    pub max_attempts: Option<i64>,
    pub state: String,
    pub release_at: Option<i64>,
    pub retire_at: Option<i64>,
    pub position: i64,
    pub requires_instance: bool,
    pub instance_config: Option<String>,
    /// `flag` (also instance backed) or `dfir` (question items, tournament events only).
    pub kind: String,
    pub created_at: i64,
}

impl ChallengeRow {
    /// Release and retire times for `kymo_core::can_submit`. A retired challenge is closed
    /// regardless of its schedule.
    pub fn window(&self) -> kymo_core::window::ChallengeWindow {
        let retire = if self.state == "retired" {
            Some(i64::MIN)
        } else {
            self.retire_at
        };
        (self.release_at, retire)
    }

    pub fn format(&self) -> kymo_jeopardy::FlagFormat {
        kymo_jeopardy::FlagFormat::new(self.flag_prefix.clone(), self.flag_suffix.clone())
    }

    pub fn attempt_cap(&self) -> kymo_jeopardy::AttemptCap {
        kymo_jeopardy::AttemptCap {
            max: self.max_attempts.and_then(|m| u32::try_from(m).ok()),
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct HintRow {
    pub id: i64,
    pub challenge_id: i64,
    pub content: String,
    pub cost: i64,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AttachmentMeta {
    pub id: i64,
    pub challenge_id: i64,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub sha256: String,
}

/// The editable part of a challenge, validated before it is written.
#[derive(Debug, Clone)]
pub struct ChallengeFields {
    pub category_id: i64,
    pub title: String,
    pub description: String,
    pub scoring: Scoring,
    pub flag: FlagSpec,
    pub flag_prefix: String,
    pub flag_suffix: String,
    pub case_sensitive: bool,
    pub max_attempts: Option<i64>,
    pub release_at: Option<i64>,
    pub retire_at: Option<i64>,
    pub position: i64,
    pub requires_instance: bool,
    pub instance_config: Option<Value>,
    pub kind: String,
}

impl From<&ChallengeRow> for ChallengeFields {
    fn from(row: &ChallengeRow) -> Self {
        ChallengeFields {
            category_id: row.category_id,
            title: row.title.clone(),
            description: row.description.clone(),
            scoring: row.scoring,
            flag: row.flag.clone(),
            flag_prefix: row.flag_prefix.clone(),
            flag_suffix: row.flag_suffix.clone(),
            case_sensitive: row.case_sensitive,
            max_attempts: row.max_attempts,
            release_at: row.release_at,
            retire_at: row.retire_at,
            position: row.position,
            requires_instance: row.requires_instance,
            instance_config: row
                .instance_config
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok()),
            kind: row.kind.clone(),
        }
    }
}

fn bad(message: &str) -> ApiError {
    ApiError::BadRequest(message.into())
}

/// Checks a release and retire pair.
pub fn check_schedule(release: Option<i64>, retire: Option<i64>) -> Result<(), ApiError> {
    for t in [release, retire].into_iter().flatten() {
        if !(EARLIEST..=LATEST).contains(&t) {
            return Err(bad("times are unix seconds between 2000 and 2100"));
        }
    }
    if let (Some(a), Some(b)) = (release, retire)
        && a >= b
    {
        return Err(bad("a challenge must be released before it is retired"));
    }
    Ok(())
}

impl ChallengeFields {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > MAX_TITLE
            || self.title.chars().any(crate::text::is_hidden_char)
        {
            return Err(bad("the title is 1 to 120 characters of plain text"));
        }
        if self.description.chars().count() > MAX_CHALLENGE_DESCRIPTION
            || self
                .description
                .chars()
                .any(|c| crate::text::is_hidden_char(c) && !matches!(c, '\n' | '\t'))
        {
            return Err(bad("the description is at most 20000 characters of text"));
        }
        self.scoring
            .validate()
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let too_big = match self.scoring {
            Scoring::Static { points } => points > MAX_POINTS,
            Scoring::Dynamic { initial, .. } => initial > MAX_POINTS,
        };
        if too_big {
            return Err(bad("points are at most 1000000"));
        }
        self.flag
            .validate()
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let text = match &self.flag {
            FlagSpec::Static { value } => value,
            FlagSpec::Regex { pattern } => pattern,
            FlagSpec::PerTeam { expected } => expected,
        };
        if text.trim().is_empty() || text.chars().count() > MAX_FLAG {
            return Err(bad("the flag is 1 to 500 characters"));
        }
        for part in [&self.flag_prefix, &self.flag_suffix] {
            if part.chars().count() > MAX_FORMAT || part.chars().any(crate::text::is_hidden_char) {
                return Err(bad("the flag prefix and suffix are at most 100 characters"));
            }
        }
        if self
            .max_attempts
            .is_some_and(|m| !(1..=10_000).contains(&m))
        {
            return Err(bad("max_attempts is 1 to 10000, or null for unlimited"));
        }
        check_schedule(self.release_at, self.retire_at)?;
        if self
            .instance_config
            .as_ref()
            .is_some_and(|v| !v.is_object() || v.to_string().len() > MAX_INSTANCE_CONFIG)
        {
            return Err(bad("instance_config is a JSON object under 16 KB"));
        }
        if self.requires_instance {
            let config = self
                .instance_config
                .clone()
                .ok_or_else(|| bad("an instance challenge needs an instance_config"))?;
            let spec: kymo_instances::InstanceSpec = serde_json::from_value(config)
                .map_err(|_| bad("instance_config needs image, port, cpu_millis, memory_mb, pids and ttl_secs"))?;
            spec.validate(&kymo_instances::ResourceCaps::default())
                .map_err(|e| bad(&e.to_string()))?;
        }
        Ok(())
    }
}

fn to_json<T: Serialize>(value: &T) -> Result<String, ApiError> {
    serde_json::to_string(value).map_err(|_| ApiError::Internal)
}

// Categories

pub async fn categories(pool: &SqlitePool, event_id: i64) -> Result<Vec<CategoryRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, event_id, name, position FROM category WHERE event_id = ? ORDER BY position, id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?)
}

pub async fn category_by_id(pool: &SqlitePool, id: i64) -> Result<CategoryRow, ApiError> {
    Ok(
        sqlx::query_as("SELECT id, event_id, name, position FROM category WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}

fn category_conflict(err: sqlx::Error) -> ApiError {
    match &err {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("this event already has a category with that name".into())
        }
        _ => err.into(),
    }
}

pub async fn category_insert(
    pool: &SqlitePool,
    event_id: i64,
    name: &str,
    position: i64,
) -> Result<i64, ApiError> {
    let done = sqlx::query("INSERT INTO category (event_id, name, position) VALUES (?, ?, ?)")
        .bind(event_id)
        .bind(name)
        .bind(position)
        .execute(pool)
        .await
        .map_err(category_conflict)?;
    Ok(done.last_insert_rowid())
}

pub async fn category_update(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    position: i64,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE category SET name = ?, position = ? WHERE id = ?")
        .bind(name)
        .bind(position)
        .bind(id)
        .execute(pool)
        .await
        .map_err(category_conflict)?;
    Ok(())
}

pub async fn category_delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let used: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenge WHERE category_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if used > 0 {
        return Err(ApiError::Conflict(
            "move or delete the challenges of this category first".into(),
        ));
    }
    sqlx::query("DELETE FROM category WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// Challenges

pub async fn challenge_by_id(pool: &SqlitePool, id: i64) -> Result<ChallengeRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM challenge WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub async fn challenges(pool: &SqlitePool, event_id: i64) -> Result<Vec<ChallengeRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM challenge WHERE event_id = ? ORDER BY position, id")
            .bind(event_id)
            .fetch_all(pool)
            .await?,
    )
}

/// The category must belong to the same event as the challenge.
async fn check_category(
    pool: &SqlitePool,
    event_id: i64,
    category_id: i64,
) -> Result<(), ApiError> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT id FROM category WHERE id = ? AND event_id = ?")
            .bind(category_id)
            .bind(event_id)
            .fetch_optional(pool)
            .await?;
    found
        .map(|_| ())
        .ok_or_else(|| bad("that category does not belong to this event"))
}

pub async fn challenge_insert(
    pool: &SqlitePool,
    event_id: i64,
    fields: &ChallengeFields,
    now: i64,
) -> Result<i64, ApiError> {
    check_category(pool, event_id, fields.category_id).await?;
    let done = sqlx::query(
        "INSERT INTO challenge (event_id, category_id, title, description, scoring, flag, flag_prefix, flag_suffix, case_sensitive, max_attempts, release_at, retire_at, position, requires_instance, instance_config, kind, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event_id)
    .bind(fields.category_id)
    .bind(&fields.title)
    .bind(&fields.description)
    .bind(to_json(&fields.scoring)?)
    .bind(to_json(&fields.flag)?)
    .bind(&fields.flag_prefix)
    .bind(&fields.flag_suffix)
    .bind(fields.case_sensitive)
    .bind(fields.max_attempts)
    .bind(fields.release_at)
    .bind(fields.retire_at)
    .bind(fields.position)
    .bind(fields.requires_instance)
    .bind(fields.instance_config.as_ref().map(Value::to_string))
    .bind(&fields.kind)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(done.last_insert_rowid())
}

pub async fn challenge_update(
    pool: &SqlitePool,
    row: &ChallengeRow,
    fields: &ChallengeFields,
) -> Result<(), ApiError> {
    check_category(pool, row.event_id, fields.category_id).await?;
    sqlx::query(
        "UPDATE challenge SET category_id = ?, title = ?, description = ?, scoring = ?, flag = ?, flag_prefix = ?, flag_suffix = ?, case_sensitive = ?, max_attempts = ?, release_at = ?, retire_at = ?, position = ?, requires_instance = ?, instance_config = ? WHERE id = ?",
    )
    .bind(fields.category_id)
    .bind(&fields.title)
    .bind(&fields.description)
    .bind(to_json(&fields.scoring)?)
    .bind(to_json(&fields.flag)?)
    .bind(&fields.flag_prefix)
    .bind(&fields.flag_suffix)
    .bind(fields.case_sensitive)
    .bind(fields.max_attempts)
    .bind(fields.release_at)
    .bind(fields.retire_at)
    .bind(fields.position)
    .bind(fields.requires_instance)
    .bind(fields.instance_config.as_ref().map(Value::to_string))
    .bind(row.id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_challenge_state(pool: &SqlitePool, id: i64, state: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE challenge SET state = ? WHERE id = ?")
        .bind(state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_schedule(
    pool: &SqlitePool,
    id: i64,
    release_at: Option<i64>,
    retire_at: Option<i64>,
) -> Result<(), ApiError> {
    check_schedule(release_at, retire_at)?;
    sqlx::query("UPDATE challenge SET release_at = ?, retire_at = ? WHERE id = ?")
        .bind(release_at)
        .bind(retire_at)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Refused once someone solved it: retire the challenge instead of erasing scores.
pub async fn challenge_delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    let solves: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM solve WHERE challenge_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if solves > 0 {
        return Err(ApiError::Conflict(
            "this challenge has solves, retire it instead of deleting it".into(),
        ));
    }
    sqlx::query("DELETE FROM challenge WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// Hints

pub fn check_hint(content: &str, cost: i64) -> Result<String, ApiError> {
    let content = content.trim();
    if content.is_empty()
        || content.chars().count() > MAX_HINT
        || content
            .chars()
            .any(|c| crate::text::is_hidden_char(c) && !matches!(c, '\n' | '\t'))
    {
        return Err(bad("a hint is 1 to 5000 characters of text"));
    }
    if !(0..=MAX_POINTS).contains(&cost) {
        return Err(bad("a hint costs 0 to 1000000 points"));
    }
    Ok(content.to_owned())
}

pub async fn hints(pool: &SqlitePool, challenge_id: i64) -> Result<Vec<HintRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, challenge_id, content, cost, position FROM hint WHERE challenge_id = ? ORDER BY position, id",
    )
    .bind(challenge_id)
    .fetch_all(pool)
    .await?)
}

pub async fn hints_of_event(pool: &SqlitePool, event_id: i64) -> Result<Vec<HintRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT h.id, h.challenge_id, h.content, h.cost, h.position FROM hint h JOIN challenge c ON c.id = h.challenge_id WHERE c.event_id = ? ORDER BY h.position, h.id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?)
}

pub async fn hint_by_id(pool: &SqlitePool, id: i64) -> Result<HintRow, ApiError> {
    Ok(
        sqlx::query_as("SELECT id, challenge_id, content, cost, position FROM hint WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn hint_insert(
    pool: &SqlitePool,
    challenge_id: i64,
    content: &str,
    cost: i64,
    position: i64,
) -> Result<i64, ApiError> {
    let done =
        sqlx::query("INSERT INTO hint (challenge_id, content, cost, position) VALUES (?, ?, ?, ?)")
            .bind(challenge_id)
            .bind(content)
            .bind(cost)
            .bind(position)
            .execute(pool)
            .await?;
    Ok(done.last_insert_rowid())
}

pub async fn hint_update(
    pool: &SqlitePool,
    id: i64,
    content: &str,
    cost: i64,
    position: i64,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE hint SET content = ?, cost = ?, position = ? WHERE id = ?")
        .bind(content)
        .bind(cost)
        .bind(position)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn hint_delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM hint WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// Attachments

/// Keeps letters, digits, dot, dash and underscore, so the name is safe in a header and on disk.
pub fn clean_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(MAX_FILENAME)
        .collect();
    let trimmed = cleaned.trim_start_matches('.');
    if trimmed.is_empty() {
        "file".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// A plain `type/subtype` media type, or the generic binary type.
pub fn clean_mime(raw: &str) -> String {
    let base = raw.split(';').next().unwrap_or("").trim().to_lowercase();
    let valid = base.len() <= 100
        && base.split_once('/').is_some_and(|(a, b)| {
            [a, b].iter().all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
            })
        });
    if valid {
        base
    } else {
        "application/octet-stream".to_owned()
    }
}

pub async fn attachment_insert(
    pool: &SqlitePool,
    challenge_id: i64,
    name: &str,
    mime: &str,
    data: &[u8],
) -> Result<AttachmentMeta, ApiError> {
    let sha = Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let size = i64::try_from(data.len()).map_err(|_| ApiError::Internal)?;
    let done = sqlx::query(
        "INSERT INTO attachment (challenge_id, name, mime, size, sha256, data) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(challenge_id)
    .bind(name)
    .bind(mime)
    .bind(size)
    .bind(&sha)
    .bind(data)
    .execute(pool)
    .await?;
    Ok(AttachmentMeta {
        id: done.last_insert_rowid(),
        challenge_id,
        name: name.to_owned(),
        mime: mime.to_owned(),
        size,
        sha256: sha,
    })
}

pub async fn attachments_of_challenge(
    pool: &SqlitePool,
    challenge_id: i64,
) -> Result<Vec<AttachmentMeta>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, challenge_id, name, mime, size, sha256 FROM attachment WHERE challenge_id = ? ORDER BY id",
    )
    .bind(challenge_id)
    .fetch_all(pool)
    .await?)
}

pub async fn attachments_of_event(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<Vec<AttachmentMeta>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT a.id, a.challenge_id, a.name, a.mime, a.size, a.sha256 FROM attachment a JOIN challenge c ON c.id = a.challenge_id WHERE c.event_id = ? ORDER BY a.id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?)
}

pub async fn attachment_meta(pool: &SqlitePool, id: i64) -> Result<AttachmentMeta, ApiError> {
    Ok(sqlx::query_as(
        "SELECT id, challenge_id, name, mime, size, sha256 FROM attachment WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

pub async fn attachment_data(pool: &SqlitePool, id: i64) -> Result<Vec<u8>, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT data FROM attachment WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn attachment_delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM attachment WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct Base64Upload {
    pub name: String,
    pub mime: Option<String>,
    pub data_base64: String,
}
