//! Storage for Jeopardy play: per event settings, submissions, solves, hint unlocks and the
//! inputs that kymo-jeopardy ranks. No scoring rule lives here.

use std::collections::{BTreeMap, HashSet};

use kymo_jeopardy::board::{BoardConfig, Challenge, Participant, Solve, TieBreak};
use kymo_jeopardy::{RateRule, solve_counts};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};

use crate::challenge_store;
use crate::error::ApiError;
use crate::event_store::{self, EventRow};
use crate::sharing::{MAX_WARNINGS, SharingAction};
use crate::team_store;
use kymo_core::Mode;

const MAX_STORED_VALUE: usize = 200;
pub const MAX_PER_PAGE: i64 = 100;

/// A DFIR question counts on the board as a challenge with this offset added to its id, worth
/// the question's points. Real challenge ids never get this large.
pub const ITEM_BASE: u64 = 1 << 40;

/// Per event scoring and submission settings. Defaults apply until an admin saves them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JeopardySettings {
    pub tie_breaks: Vec<TieBreak>,
    pub rate_max_attempts: u32,
    pub rate_window_secs: i64,
    pub rate_lockout_secs: i64,
    /// From this moment the public scoreboard and chart stop moving, until the event ends.
    /// Staff keep seeing the live board and play carries on. None means no freeze.
    #[serde(default)]
    pub freeze_at: Option<i64>,
    /// What happens to a player who submits a dynamic flag generated for somebody else.
    #[serde(default = "default_sharing_action")]
    pub sharing_action: SharingAction,
    /// Such submissions that only warn, before the action applies.
    #[serde(default = "default_sharing_warnings")]
    pub sharing_warnings: u32,
}

fn default_sharing_action() -> SharingAction {
    SharingAction::AutoBan
}

fn default_sharing_warnings() -> u32 {
    1
}

impl Default for JeopardySettings {
    fn default() -> Self {
        Self {
            tie_breaks: BoardConfig::default().tie_breaks,
            rate_max_attempts: 10,
            rate_window_secs: 60,
            rate_lockout_secs: 60,
            freeze_at: None,
            sharing_action: default_sharing_action(),
            sharing_warnings: default_sharing_warnings(),
        }
    }
}

impl JeopardySettings {
    pub fn validate(&self) -> Result<(), ApiError> {
        let bad = |m: &str| Err(ApiError::BadRequest(m.into()));
        let repeated = self
            .tie_breaks
            .iter()
            .enumerate()
            .any(|(i, t)| self.tie_breaks[..i].contains(t));
        if self.tie_breaks.len() > 3 || repeated {
            return bad("tie_breaks lists each rule at most once");
        }
        if !(1..=1000).contains(&self.rate_max_attempts) {
            return bad("rate_max_attempts is 1 to 1000");
        }
        if !(1..=86_400).contains(&self.rate_window_secs) {
            return bad("rate_window_secs is 1 to 86400");
        }
        if !(0..=86_400).contains(&self.rate_lockout_secs) {
            return bad("rate_lockout_secs is 0 to 86400");
        }
        if self
            .freeze_at
            .is_some_and(|t| !(946_684_800..=4_102_444_800).contains(&t))
        {
            return bad("freeze_at is a unix time between 2000 and 2100");
        }
        if self.sharing_warnings > MAX_WARNINGS {
            return bad("sharing_warnings is 0 to 10");
        }
        Ok(())
    }

    pub fn rate_rule(&self) -> RateRule {
        RateRule {
            max_attempts: self.rate_max_attempts,
            window_secs: self.rate_window_secs,
            lockout_secs: self.rate_lockout_secs,
        }
    }

    pub fn board_config(&self) -> BoardConfig {
        BoardConfig {
            tie_breaks: self.tie_breaks.clone(),
            ..BoardConfig::default()
        }
    }
}

#[derive(FromRow)]
struct SettingsRow {
    tie_breaks: String,
    rate_max_attempts: i64,
    rate_window_secs: i64,
    rate_lockout_secs: i64,
    freeze_at: Option<i64>,
    sharing_action: String,
    sharing_warnings: i64,
}

pub async fn settings(pool: &SqlitePool, event_id: i64) -> Result<JeopardySettings, ApiError> {
    let row: Option<SettingsRow> = sqlx::query_as(
        "SELECT tie_breaks, rate_max_attempts, rate_window_secs, rate_lockout_secs, freeze_at, sharing_action, sharing_warnings FROM event_jeopardy WHERE event_id = ?",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(JeopardySettings::default());
    };
    Ok(JeopardySettings {
        tie_breaks: serde_json::from_str(&row.tie_breaks).map_err(|_| ApiError::Internal)?,
        rate_max_attempts: u32::try_from(row.rate_max_attempts).map_err(|_| ApiError::Internal)?,
        rate_window_secs: row.rate_window_secs,
        rate_lockout_secs: row.rate_lockout_secs,
        freeze_at: row.freeze_at,
        sharing_action: serde_json::from_value(serde_json::Value::String(row.sharing_action))
            .map_err(|_| ApiError::Internal)?,
        sharing_warnings: u32::try_from(row.sharing_warnings).map_err(|_| ApiError::Internal)?,
    })
}

pub async fn save_settings(
    pool: &SqlitePool,
    event_id: i64,
    settings: &JeopardySettings,
) -> Result<(), ApiError> {
    let tie_breaks = serde_json::to_string(&settings.tie_breaks).map_err(|_| ApiError::Internal)?;
    let action = match settings.sharing_action {
        SharingAction::Off => "off",
        SharingAction::Manual => "manual",
        SharingAction::AutoBan => "auto_ban",
    };
    sqlx::query(
        "INSERT INTO event_jeopardy (event_id, tie_breaks, rate_max_attempts, rate_window_secs, rate_lockout_secs, freeze_at, sharing_action, sharing_warnings) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(event_id) DO UPDATE SET tie_breaks = ?2, rate_max_attempts = ?3, rate_window_secs = ?4, rate_lockout_secs = ?5, freeze_at = ?6, sharing_action = ?7, sharing_warnings = ?8",
    )
    .bind(event_id)
    .bind(tie_breaks)
    .bind(i64::from(settings.rate_max_attempts))
    .bind(settings.rate_window_secs)
    .bind(settings.rate_lockout_secs)
    .bind(settings.freeze_at)
    .bind(action)
    .bind(i64::from(settings.sharing_warnings))
    .execute(pool)
    .await?;
    Ok(())
}

/// The scoring participant of an event: the user in solo events, the team in team events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scorer {
    User(i64),
    Team(i64),
}

impl Scorer {
    pub fn kind(self) -> &'static str {
        match self {
            Scorer::User(_) => "user",
            Scorer::Team(_) => "team",
        }
    }

    pub fn id(self) -> i64 {
        match self {
            Scorer::User(id) | Scorer::Team(id) => id,
        }
    }

    /// Key of hint unlocks.
    pub fn key(self) -> String {
        format!("{}:{}", self.kind(), self.id())
    }
}

fn not_registered() -> ApiError {
    ApiError::Denied("register for this event first".into())
}

/// The caller's scoring participant when they are registered for the event.
pub async fn scorer_of(
    pool: &SqlitePool,
    event: &EventRow,
    user_id: i64,
) -> Result<Scorer, ApiError> {
    let (user, team) = match event.mode()? {
        Mode::Solo => (Some(user_id), None),
        Mode::Team => {
            let team = team_store::of_user(pool, user_id)
                .await?
                .ok_or_else(not_registered)?;
            (None, Some(team.id))
        }
    };
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM event_participant WHERE event_id = ?1 AND (user_id = ?2 OR team_id = ?3)",
    )
    .bind(event.id)
    .bind(user)
    .bind(team)
    .fetch_optional(pool)
    .await?;
    found.ok_or_else(not_registered)?;
    Ok(match (user, team) {
        (Some(id), _) => Scorer::User(id),
        (_, Some(id)) => Scorer::Team(id),
        _ => return Err(ApiError::Internal),
    })
}

#[derive(Debug, Clone, FromRow)]
pub struct SolveRow {
    pub challenge_id: i64,
    pub participant_id: i64,
    pub user_id: i64,
    pub at: i64,
    pub hints_cost: i64,
}

impl SolveRow {
    pub fn to_solve(&self) -> Solve {
        Solve {
            participant: self.participant_id as u64,
            challenge: self.challenge_id as u64,
            at: self.at,
            hints_cost: self.hints_cost,
        }
    }
}

/// Everything needed to rank an event, loaded together so values stay consistent.
pub struct Board {
    pub names: BTreeMap<u64, String>,
    pub participants: Vec<Participant>,
    pub challenges: Vec<Challenge>,
    pub rows: Vec<SolveRow>,
    pub solves: Vec<Solve>,
}

impl Board {
    pub async fn load(pool: &SqlitePool, event: &EventRow) -> Result<Board, ApiError> {
        let mut names = BTreeMap::new();
        let mut participants = Vec::new();
        for p in event_store::participants(pool, event.id).await? {
            if let Some(id) = p.team_id.or(p.user_id).and_then(|i| u64::try_from(i).ok()) {
                names.insert(id, p.name);
                participants.push(Participant { id });
            }
        }
        let rows_of_event = challenge_store::challenges(pool, event.id).await?;
        let all_flags = challenge_store::flag_rows_of_event(pool, event.id).await?;
        let mut challenges: Vec<Challenge> = Vec::new();
        // The points of each flag of a multi flag challenge, which count like small challenges
        // of their own (see FLAG_BASE), so partial credit shows on the board.
        let mut flag_points: BTreeMap<i64, i64> = BTreeMap::new();
        for c in rows_of_event {
            if !c.multi_flag {
                challenges.push(Challenge {
                    id: c.id as u64,
                    scoring: c.scoring,
                });
                continue;
            }
            let own: Vec<&challenge_store::FlagRow> = all_flags
                .iter()
                .filter(|f| f.challenge_id == c.id)
                .collect();
            let weights: Vec<i64> = own.iter().map(|f| f.weight).collect();
            let total = kymo_jeopardy::current_value(&c.scoring, 0);
            for (flag, share) in own
                .iter()
                .zip(kymo_jeopardy::dynamic::split_points(total, &weights))
            {
                flag_points.insert(flag.id, share);
                challenges.push(Challenge {
                    id: crate::flag_set::FLAG_BASE + flag.id as u64,
                    scoring: kymo_jeopardy::Scoring::Static { points: share },
                });
            }
            // The challenge itself is worth nothing on the board: its flags carry the points.
            challenges.push(Challenge {
                id: c.id as u64,
                scoring: kymo_jeopardy::Scoring::Static { points: 0 },
            });
        }
        let questions: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT i.id, i.points FROM dfir_item i JOIN challenge c ON c.id = i.challenge_id WHERE c.event_id = ?",
        )
        .bind(event.id)
        .fetch_all(pool)
        .await?;
        challenges.extend(questions.into_iter().map(|(id, points)| Challenge {
            id: ITEM_BASE + id as u64,
            scoring: kymo_jeopardy::Scoring::Static { points },
        }));
        let rows: Vec<SolveRow> = sqlx::query_as(
            "SELECT challenge_id, participant_id, user_id, at, hints_cost FROM solve WHERE event_id = ? ORDER BY id",
        )
        .bind(event.id)
        .fetch_all(pool)
        .await?;
        let mut solves: Vec<Solve> = rows.iter().map(SolveRow::to_solve).collect();
        let answered: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT item_id, participant_id, at FROM item_solve WHERE event_id = ? ORDER BY id",
        )
        .bind(event.id)
        .fetch_all(pool)
        .await?;
        solves.extend(answered.into_iter().map(|(item, participant, at)| Solve {
            participant: participant as u64,
            challenge: ITEM_BASE + item as u64,
            at,
            hints_cost: 0,
        }));
        let found: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT flag_id, participant_id, at, points FROM flag_solve WHERE event_id = ? ORDER BY id",
        )
        .bind(event.id)
        .fetch_all(pool)
        .await?;
        solves.extend(found.into_iter().map(|(flag, participant, at, points)| {
            let base = flag_points.get(&flag).copied().unwrap_or(points);
            Solve {
                participant: participant as u64,
                challenge: crate::flag_set::FLAG_BASE + flag as u64,
                at,
                // What hint costs took from this flag's share, so the board shows the award.
                hints_cost: (base - points).max(0),
            }
        }));
        Ok(Board {
            names,
            participants,
            challenges,
            rows,
            solves,
        })
    }

    /// Drops every solve after `cutoff`, so the board shows the state at that moment.
    pub fn freeze_at(&mut self, cutoff: i64) {
        self.rows.retain(|r| r.at <= cutoff);
        self.solves.retain(|s| s.at <= cutoff);
    }

    /// Counted solves per challenge, the input of the current point value.
    pub fn counts(&self) -> BTreeMap<u64, u32> {
        solve_counts(&self.participants, &self.challenges, &self.solves)
    }

    pub fn solves_of(&self, participant: i64) -> impl Iterator<Item = &SolveRow> {
        self.rows
            .iter()
            .filter(move |r| r.participant_id == participant)
    }
}

pub async fn is_solved(
    pool: &SqlitePool,
    challenge_id: i64,
    participant_id: i64,
) -> Result<bool, ApiError> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT id FROM solve WHERE challenge_id = ? AND participant_id = ?")
            .bind(challenge_id)
            .bind(participant_id)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

/// Counted attempts so far (bad format submissions do not count).
pub async fn attempts_used(
    pool: &SqlitePool,
    challenge_id: i64,
    scorer: Scorer,
) -> Result<u32, ApiError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM submission WHERE challenge_id = ? AND participant_kind = ? AND participant_id = ? AND verdict <> 'bad_format'",
    )
    .bind(challenge_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .fetch_one(pool)
    .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

pub async fn unlocked_hints(pool: &SqlitePool, scorer: Scorer) -> Result<HashSet<i64>, ApiError> {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT hint_id FROM hint_unlock WHERE participant_key = ?")
            .bind(scorer.key())
            .fetch_all(pool)
            .await?;
    Ok(ids.into_iter().collect())
}

/// Records the unlock once. Returns false when it already existed.
pub async fn unlock_hint(
    pool: &SqlitePool,
    scorer: Scorer,
    hint_id: i64,
    now: i64,
) -> Result<bool, ApiError> {
    let done = sqlx::query(
        "INSERT OR IGNORE INTO hint_unlock (participant_key, hint_id, at) VALUES (?, ?, ?)",
    )
    .bind(scorer.key())
    .bind(hint_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Total cost of the hints this participant unlocked on the challenge.
pub async fn hints_cost(
    tx: &mut Transaction<'_, Sqlite>,
    challenge_id: i64,
    scorer: Scorer,
) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(h.cost), 0) FROM hint_unlock u JOIN hint h ON h.id = u.hint_id WHERE h.challenge_id = ? AND u.participant_key = ?",
    )
    .bind(challenge_id)
    .bind(scorer.key())
    .fetch_one(&mut **tx)
    .await?)
}

pub struct NewSubmission<'a> {
    pub event_id: i64,
    pub challenge_id: i64,
    /// The DFIR question answered, if this was one.
    pub item_id: Option<i64>,
    /// The flag of a multi flag challenge this submission matched, if any.
    pub flag_id: Option<i64>,
    pub scorer: Scorer,
    pub user_id: i64,
    pub ip: &'a str,
    pub value: &'a str,
    pub verdict: &'a str,
    pub at: i64,
}

pub async fn insert_submission(
    tx: &mut Transaction<'_, Sqlite>,
    s: &NewSubmission<'_>,
) -> Result<(), ApiError> {
    let value: String = s.value.chars().take(MAX_STORED_VALUE).collect();
    sqlx::query(
        "INSERT INTO submission (event_id, challenge_id, item_id, flag_id, participant_kind, participant_id, user_id, ip, value, verdict, at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.event_id)
    .bind(s.challenge_id)
    .bind(s.item_id)
    .bind(s.flag_id)
    .bind(s.scorer.kind())
    .bind(s.scorer.id())
    .bind(s.user_id)
    .bind(s.ip)
    .bind(value)
    .bind(s.verdict)
    .bind(s.at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_solve(
    tx: &mut Transaction<'_, Sqlite>,
    s: &NewSubmission<'_>,
    hints_cost: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO solve (event_id, challenge_id, participant_id, user_id, at, hints_cost) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(s.event_id)
    .bind(s.challenge_id)
    .bind(s.scorer.id())
    .bind(s.user_id)
    .bind(s.at)
    .bind(hints_cost)
    .execute(&mut **tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("already solved".into())
        }
        _ => e.into(),
    })?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct HistoryFilter {
    pub user_id: Option<i64>,
    pub challenge_id: Option<i64>,
    pub verdict: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct SubmissionOut {
    pub id: i64,
    pub challenge_id: i64,
    pub challenge_title: String,
    pub participant_kind: String,
    pub participant_id: i64,
    pub user_id: i64,
    pub username: Option<String>,
    pub ip: String,
    /// Empty when the value is a correct flag: staff lists never show answers, see `hidden`.
    pub value: String,
    pub verdict: String,
    pub at: i64,
    /// True when `value` was left out because it is a correct flag.
    #[sqlx(skip)]
    pub hidden: bool,
}

#[derive(Debug, Serialize)]
pub struct SubmissionPage {
    pub items: Vec<SubmissionOut>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
}

const HISTORY_WHERE: &str = "s.event_id = ?1 AND (?2 IS NULL OR s.user_id = ?2) AND (?3 IS NULL OR s.challenge_id = ?3) AND (?4 IS NULL OR s.verdict = ?4) AND (?5 IS NULL OR s.ip = ?5)";

pub async fn history(
    pool: &SqlitePool,
    event_id: i64,
    filter: &HistoryFilter,
    page: i64,
    per_page: i64,
) -> Result<SubmissionPage, ApiError> {
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM submission s WHERE {HISTORY_WHERE}"
    ))
    .bind(event_id)
    .bind(filter.user_id)
    .bind(filter.challenge_id)
    .bind(filter.verdict.as_deref())
    .bind(filter.ip.as_deref())
    .fetch_one(pool)
    .await?;
    let mut items: Vec<SubmissionOut> = sqlx::query_as(&format!(
        "SELECT s.id, s.challenge_id, c.title AS challenge_title, s.participant_kind, s.participant_id, s.user_id, u.username, s.ip, s.value, s.verdict, s.at \
         FROM submission s JOIN challenge c ON c.id = s.challenge_id LEFT JOIN user u ON u.id = s.user_id \
         WHERE {HISTORY_WHERE} ORDER BY s.id DESC LIMIT ?6 OFFSET ?7"
    ))
    .bind(event_id)
    .bind(filter.user_id)
    .bind(filter.challenge_id)
    .bind(filter.verdict.as_deref())
    .bind(filter.ip.as_deref())
    .bind(per_page)
    .bind((page - 1).saturating_mul(per_page))
    .fetch_all(pool)
    .await?;
    // A correct submission is the answer itself. Listing it would hand the flag to anyone who
    // can read the log, or who watches a screen share, so it is revealed one row at a time.
    for item in &mut items {
        if item.verdict == "correct" {
            item.value.clear();
            item.hidden = true;
        }
    }
    Ok(SubmissionPage {
        items,
        page,
        per_page,
        total,
    })
}

/// The submitted text of one submission, for an audited reveal.
pub async fn submission_value(
    pool: &SqlitePool,
    event_id: i64,
    id: i64,
) -> Result<String, ApiError> {
    sqlx::query_scalar("SELECT value FROM submission WHERE id = ? AND event_id = ?")
        .bind(id)
        .bind(event_id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Solves per challenge for the admin listing.
pub async fn solve_totals(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<BTreeMap<i64, i64>, ApiError> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT challenge_id, COUNT(*) FROM solve WHERE event_id = ? GROUP BY challenge_id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Counted wrong or right answers to one DFIR question so far.
pub async fn item_attempts_used(
    pool: &SqlitePool,
    item_id: i64,
    scorer: Scorer,
) -> Result<u32, ApiError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM submission WHERE item_id = ? AND participant_kind = ? AND participant_id = ? AND verdict <> 'bad_format'",
    )
    .bind(item_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .fetch_one(pool)
    .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The questions this participant has answered correctly, with when.
pub async fn items_solved(
    pool: &SqlitePool,
    event_id: i64,
    scorer: Scorer,
) -> Result<std::collections::HashMap<i64, i64>, ApiError> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT item_id, at FROM item_solve WHERE event_id = ? AND participant_id = ?",
    )
    .bind(event_id)
    .bind(scorer.id())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

pub async fn insert_item_solve(
    tx: &mut Transaction<'_, Sqlite>,
    s: &NewSubmission<'_>,
    points: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO item_solve (event_id, challenge_id, item_id, participant_id, user_id, points, at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.event_id)
    .bind(s.challenge_id)
    .bind(s.item_id)
    .bind(s.scorer.id())
    .bind(s.user_id)
    .bind(points)
    .bind(s.at)
    .execute(&mut **tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("already answered correctly".into())
        }
        _ => e.into(),
    })?;
    Ok(())
}
