//! Storage and validation of Jeopardy categories, challenges, hints and attachments.
//! Rules about scoring and flags live in kymo-jeopardy; this module only checks and stores.

use kymo_jeopardy::dynamic::{self, DEFAULT_RANDOM_LEN, MAX_FLAGS, MAX_WEIGHT};
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
    /// `static` (the flag above) or `dynamic` (generated for every run, see `challenge_flag`).
    pub flag_mode: String,
    /// More than one flag: the flags are rows of `challenge_flag`.
    pub multi_flag: bool,
    /// A Responder: players fix the machine and a Verifier releases the flag.
    pub responder: bool,
}

impl ChallengeRow {
    /// The flags come from `challenge_flag` and the instance runs, not from the `flag` column.
    pub fn uses_flag_set(&self) -> bool {
        self.flag_mode == "dynamic" || self.multi_flag
    }

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

// Dynamic and multi flag settings

const MAX_FLAG_NAME: usize = 32;
const MAX_FLAG_LABEL: usize = 60;

/// One flag of a challenge that has a flag set.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FlagRow {
    pub id: i64,
    pub challenge_id: i64,
    pub position: i64,
    pub name: String,
    pub label: String,
    pub value: String,
    pub random_len: i64,
    pub closing: String,
    pub weight: i64,
}

fn default_random_len() -> i64 {
    DEFAULT_RANDOM_LEN as i64
}

fn one() -> i64 {
    1
}

/// What an admin sends for one flag. `value` is the whole flag when the mode is static and
/// the prefix when it is dynamic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagInput {
    pub name: String,
    #[serde(default)]
    pub label: String,
    pub value: String,
    #[serde(default = "default_random_len")]
    pub random_len: i64,
    #[serde(default)]
    pub closing: String,
    #[serde(default = "one")]
    pub weight: i64,
}

impl From<&FlagRow> for FlagInput {
    fn from(row: &FlagRow) -> Self {
        FlagInput {
            name: row.name.clone(),
            label: row.label.clone(),
            value: row.value.clone(),
            random_len: row.random_len,
            closing: row.closing.clone(),
            weight: row.weight,
        }
    }
}

/// The dynamic, multi flag and Responder settings of a challenge, validated together with the
/// rest of it. The default is the classic single static flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagSet {
    pub mode: String,
    pub responder: bool,
    pub flags: Vec<FlagInput>,
}

impl Default for FlagSet {
    fn default() -> Self {
        FlagSet {
            mode: "static".into(),
            responder: false,
            flags: Vec::new(),
        }
    }
}

impl FlagSet {
    pub fn from_rows(row: &ChallengeRow, flags: &[FlagRow]) -> Self {
        FlagSet {
            mode: row.flag_mode.clone(),
            responder: row.responder,
            flags: flags.iter().map(FlagInput::from).collect(),
        }
    }

    /// More than one flag.
    pub fn multi(&self) -> bool {
        self.flags.len() > 1
    }

    /// The flags come from `challenge_flag`, not from the challenge's own flag.
    pub fn uses_rows(&self) -> bool {
        self.mode == "dynamic" || self.multi()
    }

    /// Checks the settings against the rest of the challenge. `tournament` is true when the
    /// event is a tournament, where only plain single static flags are allowed.
    pub fn validate(&self, fields: &ChallengeFields, tournament: bool) -> Result<(), ApiError> {
        if !matches!(self.mode.as_str(), "static" | "dynamic") {
            return Err(bad("the flag mode is static or dynamic"));
        }
        let special = self.mode == "dynamic" || self.responder || self.flags.len() > 1;
        if !special {
            return Ok(());
        }
        if tournament {
            return Err(bad(
                "dynamic flags, several flags and Responder challenges are for Jeopardy events",
            ));
        }
        if fields.kind != "flag" {
            return Err(bad("only a single flag challenge can have these settings"));
        }
        if self.mode == "dynamic" && !fields.requires_instance {
            return Err(bad("dynamic flags need an instance"));
        }
        if self.responder {
            if !fields.requires_instance {
                return Err(bad("a Responder needs an instance"));
            }
            if self.flags.len() > 1 {
                return Err(bad("a Responder has exactly one flag"));
            }
            if self.mode == "static" && !matches!(fields.flag, FlagSpec::Static { .. }) {
                return Err(bad("a static Responder flag must be a plain flag"));
            }
        }
        if !self.uses_rows() {
            return Ok(());
        }
        if self.flags.is_empty() || self.flags.len() > MAX_FLAGS {
            return Err(bad("a challenge has 1 to 8 flags"));
        }
        if self.multi() && !matches!(fields.scoring, Scoring::Static { .. }) {
            return Err(bad(
                "a challenge with several flags uses static points, split by weight",
            ));
        }
        let format =
            kymo_jeopardy::FlagFormat::new(fields.flag_prefix.clone(), fields.flag_suffix.clone());
        let mut seen = std::collections::BTreeSet::new();
        for flag in &self.flags {
            check_flag_input(flag, &self.mode, &format, fields.case_sensitive)?;
            if !seen.insert(flag.name.as_str()) {
                return Err(bad("flag names must be different from each other"));
            }
        }
        Ok(())
    }
}

fn check_flag_input(
    flag: &FlagInput,
    mode: &str,
    format: &kymo_jeopardy::FlagFormat,
    case_sensitive: bool,
) -> Result<(), ApiError> {
    let name_ok = !flag.name.is_empty()
        && flag.name.len() <= MAX_FLAG_NAME
        && flag
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !name_ok {
        return Err(bad(
            "a flag name is 1 to 32 characters: lowercase letters, digits and underscores",
        ));
    }
    if flag.label.chars().count() > MAX_FLAG_LABEL
        || flag.label.chars().any(crate::text::is_hidden_char)
    {
        return Err(bad("a flag label is at most 60 characters of plain text"));
    }
    if !(1..=MAX_WEIGHT).contains(&flag.weight) {
        return Err(bad("a flag weight is 1 to 1000"));
    }
    if flag.value.chars().any(crate::text::is_hidden_char)
        || flag.closing.chars().any(crate::text::is_hidden_char)
    {
        return Err(bad("a flag is plain text"));
    }
    if mode == "dynamic" {
        let random_len = usize::try_from(flag.random_len).unwrap_or(0);
        dynamic::validate_shape(
            &flag.value,
            &flag.closing,
            random_len,
            format,
            case_sensitive,
        )
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    } else {
        let shown = if case_sensitive {
            flag.value.clone()
        } else {
            flag.value.to_lowercase()
        };
        let fold = |s: &str| {
            if case_sensitive {
                s.to_owned()
            } else {
                s.to_lowercase()
            }
        };
        if flag.value.trim().is_empty() || flag.value.chars().count() > MAX_FLAG {
            return Err(bad("a flag is 1 to 500 characters"));
        }
        if !shown.starts_with(&fold(&format.prefix)) || !shown.ends_with(&fold(&format.suffix)) {
            return Err(bad(
                "a flag must fit the accepted prefix and suffix, or nobody can submit it",
            ));
        }
    }
    Ok(())
}

/// The flags of every challenge of an event, for the admin listing.
pub async fn flag_rows_of_event(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<Vec<FlagRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT f.* FROM challenge_flag f JOIN challenge c ON c.id = f.challenge_id WHERE c.event_id = ? ORDER BY f.challenge_id, f.position, f.id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?)
}

pub async fn flag_rows(pool: &SqlitePool, challenge_id: i64) -> Result<Vec<FlagRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM challenge_flag WHERE challenge_id = ? ORDER BY position, id")
            .bind(challenge_id)
            .fetch_all(pool)
            .await?,
    )
}

/// Whether a run or a solve exists, after which the flag settings can no longer change (only
/// the labels).
pub async fn flags_locked(pool: &SqlitePool, challenge_id: i64) -> Result<bool, ApiError> {
    let used: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM instance WHERE challenge_id = ?1) + (SELECT COUNT(*) FROM solve WHERE challenge_id = ?1) + (SELECT COUNT(*) FROM flag_solve WHERE challenge_id = ?1)",
    )
    .bind(challenge_id)
    .fetch_one(pool)
    .await?;
    Ok(used > 0)
}

/// Writes the settings of a challenge. Once the challenge is locked only labels may differ;
/// anything else is refused, so a half played challenge cannot end up inconsistent.
pub async fn apply_flag_set(
    pool: &SqlitePool,
    challenge_id: i64,
    set: &FlagSet,
) -> Result<(), ApiError> {
    let row = challenge_by_id(pool, challenge_id).await?;
    let current = FlagSet::from_rows(&row, &flag_rows(pool, challenge_id).await?);
    let wanted = if set.uses_rows() {
        set.clone()
    } else {
        FlagSet {
            flags: Vec::new(),
            ..set.clone()
        }
    };
    if current == wanted {
        return Ok(());
    }
    if flags_locked(pool, challenge_id).await? {
        return apply_labels_only(pool, challenge_id, &current, &wanted).await;
    }
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query("UPDATE challenge SET flag_mode = ?, multi_flag = ?, responder = ? WHERE id = ?")
        .bind(&wanted.mode)
        .bind(wanted.multi())
        .bind(wanted.responder)
        .bind(challenge_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM challenge_flag WHERE challenge_id = ?")
        .bind(challenge_id)
        .execute(&mut *tx)
        .await?;
    for (position, flag) in wanted.flags.iter().enumerate() {
        sqlx::query(
            "INSERT INTO challenge_flag (challenge_id, position, name, label, value, random_len, closing, weight) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(challenge_id)
        .bind(position as i64)
        .bind(&flag.name)
        .bind(&flag.label)
        .bind(&flag.value)
        .bind(flag.random_len)
        .bind(&flag.closing)
        .bind(flag.weight)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn apply_labels_only(
    pool: &SqlitePool,
    challenge_id: i64,
    current: &FlagSet,
    wanted: &FlagSet,
) -> Result<(), ApiError> {
    let strip = |set: &FlagSet| FlagSet {
        flags: set
            .flags
            .iter()
            .map(|f| FlagInput {
                label: String::new(),
                ..f.clone()
            })
            .collect(),
        ..set.clone()
    };
    if strip(current) != strip(wanted) {
        return Err(ApiError::Conflict(
            "the flags can no longer change once the challenge has a run or a solve, only their labels".into(),
        ));
    }
    for flag in &wanted.flags {
        sqlx::query("UPDATE challenge_flag SET label = ? WHERE challenge_id = ? AND name = ?")
            .bind(&flag.label)
            .bind(challenge_id)
            .bind(&flag.name)
            .execute(pool)
            .await?;
    }
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
