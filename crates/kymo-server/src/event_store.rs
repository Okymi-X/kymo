use std::str::FromStr;

use kymo_core::{EventKind, EventSchedule, EventState, Mode, TeamRules, effective_state};
use serde::Serialize;
use sqlx::{FromRow, Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::error::ApiError;

const EARLIEST: i64 = 946_684_800;
const LATEST: i64 = 4_102_444_800;
const MAX_TEAM_SIZE: i64 = 50;
pub const MAX_NAME: usize = 80;
pub const MAX_DESCRIPTION: usize = 2000;

#[derive(Debug, Clone, FromRow)]
pub struct EventRow {
    pub id: i64,
    pub kind: String,
    pub mode: String,
    pub name: String,
    pub slug: String,
    pub state: String,
    pub registration_opens: Option<i64>,
    pub registration_closes: Option<i64>,
    pub starts_at: Option<i64>,
    pub ends_at: Option<i64>,
    pub scoreboard_public: bool,
    pub description: String,
    pub team_min: i64,
    pub team_max: i64,
    pub allow_invites: bool,
    pub roster_locked: bool,
    /// `published` (a host port per instance) or `vpn` (a private lab reached through OpenVPN).
    pub network_mode: String,
    /// The lab host that runs this event's instances; none means this machine's own runtime.
    pub lab_host_id: Option<i64>,
    pub created_at: i64,
}

impl EventRow {
    pub fn schedule(&self) -> EventSchedule {
        EventSchedule {
            registration_opens: self.registration_opens,
            registration_closes: self.registration_closes,
            starts: self.starts_at,
            ends: self.ends_at,
        }
    }

    pub fn stored_state(&self) -> Result<EventState, ApiError> {
        EventState::from_str(&self.state).map_err(|_| ApiError::Internal)
    }

    pub fn effective_state(&self, now: i64) -> Result<EventState, ApiError> {
        Ok(effective_state(self.stored_state()?, &self.schedule(), now))
    }

    pub fn mode(&self) -> Result<Mode, ApiError> {
        Mode::from_str(&self.mode).map_err(|_| ApiError::Internal)
    }

    /// Roster rules with the mode applied: a solo event always has size one and no invites.
    pub fn team_rules(&self) -> Result<TeamRules, ApiError> {
        let configured = TeamRules {
            min_size: u32::try_from(self.team_min).unwrap_or(1),
            max_size: u32::try_from(self.team_max).unwrap_or(1),
            allow_invites: self.allow_invites,
            roster_locked_after_start: self.roster_locked,
        };
        Ok(TeamRules::for_mode(self.mode()?, configured))
    }
}

/// An event as the API shows it.
#[derive(Debug, Serialize)]
pub struct EventOut {
    pub id: i64,
    pub kind: String,
    pub mode: String,
    pub name: String,
    pub slug: String,
    pub state: String,
    pub effective_state: EventState,
    pub registration_opens: Option<i64>,
    pub registration_closes: Option<i64>,
    pub starts_at: Option<i64>,
    pub ends_at: Option<i64>,
    pub scoreboard_public: bool,
    pub description: String,
    pub team_min: i64,
    pub team_max: i64,
    pub allow_invites: bool,
    pub roster_locked: bool,
    pub network_mode: String,
    pub lab_host_id: Option<i64>,
    pub created_at: i64,
}

impl EventOut {
    pub fn new(row: EventRow, now: i64) -> Result<Self, ApiError> {
        Ok(EventOut {
            effective_state: row.effective_state(now)?,
            id: row.id,
            kind: row.kind,
            mode: row.mode,
            name: row.name,
            slug: row.slug,
            state: row.state,
            registration_opens: row.registration_opens,
            registration_closes: row.registration_closes,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            scoreboard_public: row.scoreboard_public,
            description: row.description,
            team_min: row.team_min,
            team_max: row.team_max,
            allow_invites: row.allow_invites,
            roster_locked: row.roster_locked,
            network_mode: row.network_mode,
            lab_host_id: row.lab_host_id,
            created_at: row.created_at,
        })
    }
}

/// The editable part of an event, validated before it is written.
#[derive(Debug, Clone)]
pub struct EventFields {
    pub kind: EventKind,
    pub mode: Mode,
    pub name: String,
    pub description: String,
    pub registration_opens: Option<i64>,
    pub registration_closes: Option<i64>,
    pub starts_at: Option<i64>,
    pub ends_at: Option<i64>,
    pub scoreboard_public: bool,
    pub team_min: i64,
    pub team_max: i64,
    pub allow_invites: bool,
    pub roster_locked: bool,
}

impl EventFields {
    pub fn validate(&self) -> Result<(), ApiError> {
        let bad = |m: &str| Err(ApiError::BadRequest(m.into()));
        for t in [
            self.registration_opens,
            self.registration_closes,
            self.starts_at,
            self.ends_at,
        ]
        .into_iter()
        .flatten()
        {
            if !(EARLIEST..=LATEST).contains(&t) {
                return bad("times are unix seconds between 2000 and 2100");
            }
        }
        if let (Some(a), Some(b)) = (self.registration_opens, self.registration_closes)
            && a >= b
        {
            return bad("registration must open before it closes");
        }
        if let (Some(a), Some(b)) = (self.starts_at, self.ends_at)
            && a >= b
        {
            return bad("the event must start before it ends");
        }
        if !(1..=MAX_TEAM_SIZE).contains(&self.team_min)
            || !(1..=MAX_TEAM_SIZE).contains(&self.team_max)
            || self.team_min > self.team_max
        {
            return bad("team sizes are 1 to 50 and the minimum cannot exceed the maximum");
        }
        if self.description.chars().count() > MAX_DESCRIPTION
            || self
                .description
                .chars()
                .any(|c| crate::text::is_hidden_char(c) && c != '\n')
        {
            return bad("the description is at most 2000 characters of plain text");
        }
        Ok(())
    }
}

impl From<&EventRow> for EventFields {
    fn from(row: &EventRow) -> Self {
        EventFields {
            kind: EventKind::from_str(&row.kind).unwrap_or(EventKind::Tournament),
            mode: Mode::from_str(&row.mode).unwrap_or(Mode::Team),
            name: row.name.clone(),
            description: row.description.clone(),
            registration_opens: row.registration_opens,
            registration_closes: row.registration_closes,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            scoreboard_public: row.scoreboard_public,
            team_min: row.team_min,
            team_max: row.team_max,
            allow_invites: row.allow_invites,
            roster_locked: row.roster_locked,
        }
    }
}

/// Picks `base`, `base-2`, `base-3`... until the slug is free.
async fn free_slug(conn: &mut SqliteConnection, base: &str) -> Result<String, ApiError> {
    let mut candidate = base.to_owned();
    let mut n = 2;
    loop {
        let taken: Option<i64> = sqlx::query_scalar("SELECT id FROM event WHERE slug = ?")
            .bind(&candidate)
            .fetch_optional(&mut *conn)
            .await?;
        if taken.is_none() {
            return Ok(candidate);
        }
        candidate = format!("{base}-{n}");
        n += 1;
    }
}

pub async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    fields: &EventFields,
    slug_base: &str,
    now: i64,
) -> Result<i64, ApiError> {
    let slug = free_slug(tx, slug_base).await?;
    let done = sqlx::query(
        "INSERT INTO event (kind, mode, name, slug, registration_opens, registration_closes, starts_at, ends_at, scoreboard_public, description, team_min, team_max, allow_invites, roster_locked, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(fields.kind.as_str())
    .bind(fields.mode.as_str())
    .bind(&fields.name)
    .bind(&slug)
    .bind(fields.registration_opens)
    .bind(fields.registration_closes)
    .bind(fields.starts_at)
    .bind(fields.ends_at)
    .bind(fields.scoreboard_public)
    .bind(&fields.description)
    .bind(fields.team_min)
    .bind(fields.team_max)
    .bind(fields.allow_invites)
    .bind(fields.roster_locked)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(done.last_insert_rowid())
}

/// The event row a new tournament gets: team mode, defaults everywhere else.
pub fn tournament_fields(name: &str) -> EventFields {
    EventFields {
        kind: EventKind::Tournament,
        mode: Mode::Team,
        name: name.to_owned(),
        description: String::new(),
        registration_opens: None,
        registration_closes: None,
        starts_at: None,
        ends_at: None,
        scoreboard_public: false,
        team_min: 1,
        team_max: 4,
        allow_invites: true,
        roster_locked: true,
    }
}

pub async fn update(pool: &SqlitePool, id: i64, fields: &EventFields) -> Result<(), ApiError> {
    let done = sqlx::query(
        "UPDATE event SET mode = ?, name = ?, registration_opens = ?, registration_closes = ?, starts_at = ?, ends_at = ?, scoreboard_public = ?, description = ?, team_min = ?, team_max = ?, allow_invites = ?, roster_locked = ? WHERE id = ?",
    )
    .bind(fields.mode.as_str())
    .bind(&fields.name)
    .bind(fields.registration_opens)
    .bind(fields.registration_closes)
    .bind(fields.starts_at)
    .bind(fields.ends_at)
    .bind(fields.scoreboard_public)
    .bind(&fields.description)
    .bind(fields.team_min)
    .bind(fields.team_max)
    .bind(fields.allow_invites)
    .bind(fields.roster_locked)
    .bind(id)
    .execute(pool)
    .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn set_state(pool: &SqlitePool, id: i64, state: EventState) -> Result<(), ApiError> {
    sqlx::query("UPDATE event SET state = ? WHERE id = ?")
        .bind(state.as_str())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn by_id(pool: &SqlitePool, id: i64) -> Result<EventRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM event WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

pub async fn by_slug(pool: &SqlitePool, slug: &str) -> Result<EventRow, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM event WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await?)
}

/// `public` hides drafts and archived events.
pub async fn list(pool: &SqlitePool, public: bool) -> Result<Vec<EventRow>, ApiError> {
    let sql = if public {
        "SELECT * FROM event WHERE state NOT IN ('draft', 'archived') ORDER BY id DESC"
    } else {
        "SELECT * FROM event ORDER BY id DESC"
    };
    Ok(sqlx::query_as(sql).fetch_all(pool).await?)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM event WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn participant_count(pool: &SqlitePool, event_id: i64) -> Result<i64, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM event_participant WHERE event_id = ?")
            .bind(event_id)
            .fetch_one(pool)
            .await?,
    )
}

#[derive(Debug, Serialize, FromRow)]
pub struct Participant {
    pub id: i64,
    pub user_id: Option<i64>,
    pub team_id: Option<i64>,
    pub name: String,
    pub members: i64,
    pub registered_at: i64,
}

pub async fn participants(pool: &SqlitePool, event_id: i64) -> Result<Vec<Participant>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT p.id, p.user_id, p.team_id, COALESCE(u.display_name, t.name) AS name, \
         CASE WHEN p.team_id IS NULL THEN 1 ELSE (SELECT COUNT(*) FROM player_team_member m WHERE m.team_id = p.team_id) END AS members, \
         p.registered_at \
         FROM event_participant p LEFT JOIN user u ON u.id = p.user_id LEFT JOIN player_team t ON t.id = p.team_id \
         WHERE p.event_id = ? ORDER BY p.id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?)
}

/// Events a team is registered in, which bound what its roster may do.
pub async fn of_team(pool: &SqlitePool, team_id: i64) -> Result<Vec<EventRow>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT e.* FROM event e JOIN event_participant p ON p.event_id = e.id WHERE p.team_id = ?",
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?)
}

pub async fn register(
    pool: &SqlitePool,
    event_id: i64,
    user_id: Option<i64>,
    team_id: Option<i64>,
    now: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO event_participant (event_id, user_id, team_id, registered_at) VALUES (?, ?, ?, ?)",
    )
    .bind(event_id)
    .bind(user_id)
    .bind(team_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApiError::Conflict("already registered for this event".into())
        }
        _ => e.into(),
    })?;
    Ok(())
}

pub async fn unregister(
    pool: &SqlitePool,
    event_id: i64,
    user_id: Option<i64>,
    team_id: Option<i64>,
) -> Result<(), ApiError> {
    let done = sqlx::query(
        "DELETE FROM event_participant WHERE event_id = ?1 AND (user_id = ?2 OR team_id = ?3)",
    )
    .bind(event_id)
    .bind(user_id)
    .bind(team_id)
    .execute(pool)
    .await?;
    if done.rows_affected() == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

/// True when a tournament row points at this event.
pub async fn has_tournament(pool: &SqlitePool, event_id: i64) -> Result<bool, ApiError> {
    let found: Option<i64> = sqlx::query_scalar("SELECT id FROM tournament WHERE event_id = ?")
        .bind(event_id)
        .fetch_optional(pool)
        .await?;
    Ok(found.is_some())
}

pub async fn set_scoreboard_public(
    pool: &SqlitePool,
    id: i64,
    public: bool,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE event SET scoreboard_public = ? WHERE id = ?")
        .bind(public)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub const NETWORK_MODES: [&str; 2] = ["published", "vpn"];

pub async fn set_network_mode(pool: &SqlitePool, id: i64, mode: &str) -> Result<(), ApiError> {
    if !NETWORK_MODES.contains(&mode) {
        return Err(ApiError::BadRequest(
            "network_mode is published or vpn".into(),
        ));
    }
    sqlx::query("UPDATE event SET network_mode = ? WHERE id = ?")
        .bind(mode)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Picks the lab host that runs an event's instances, or none for this machine.
pub async fn set_lab_host(
    pool: &SqlitePool,
    id: i64,
    lab_host_id: Option<i64>,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE event SET lab_host_id = ? WHERE id = ?")
        .bind(lab_host_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
