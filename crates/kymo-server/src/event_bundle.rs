//! Portable backup of one Jeopardy event: its settings, categories, challenges with flags,
//! hints and attachments. Players, teams, solves and submissions are not included; the full
//! database snapshot covers those. A bundle holds flags, so only people who may manage
//! challenges can export or import one.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use kymo_core::{EventKind, Mode};
use kymo_jeopardy::{FlagSpec, Scoring};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::challenge_store::{self as store, ChallengeFields};
use crate::error::ApiError;
use crate::event_store::{self, EventFields, EventRow, MAX_NAME};
use crate::jeopardy_store::{self, JeopardySettings};
use crate::text::{clean_name, slugify};

pub const FORMAT: &str = "kymo-event";
pub const VERSION: u32 = 1;
const MAX_CATEGORIES: usize = 50;
const MAX_CHALLENGES: usize = 500;
const MAX_HINTS: usize = 20;
const MAX_ATTACHMENTS: usize = 20;
const MAX_ATTACHMENT: usize = 10 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub exported_at: i64,
    pub event: EventPart,
    pub settings: JeopardySettings,
    pub categories: Vec<CategoryPart>,
    pub challenges: Vec<ChallengePart>,
}

fn published() -> String {
    "published".into()
}

#[derive(Serialize, Deserialize)]
pub struct EventPart {
    pub name: String,
    pub description: String,
    pub mode: String,
    pub team_min: i64,
    pub team_max: i64,
    pub allow_invites: bool,
    pub roster_locked: bool,
    pub scoreboard_public: bool,
    /// Older bundles have no network mode and import as published.
    #[serde(default = "published")]
    pub network_mode: String,
    pub registration_opens: Option<i64>,
    pub registration_closes: Option<i64>,
    pub starts_at: Option<i64>,
    pub ends_at: Option<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct CategoryPart {
    pub name: String,
    pub position: i64,
}

#[derive(Serialize, Deserialize)]
pub struct HintPart {
    pub content: String,
    pub cost: i64,
    pub position: i64,
}

#[derive(Serialize, Deserialize)]
pub struct AttachmentPart {
    pub name: String,
    pub mime: String,
    pub sha256: String,
    pub data_base64: String,
}

#[derive(Serialize, Deserialize)]
pub struct ChallengePart {
    pub category: String,
    pub title: String,
    pub description: String,
    pub scoring: Scoring,
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
    pub instance_config: Option<Value>,
    pub hints: Vec<HintPart>,
    pub attachments: Vec<AttachmentPart>,
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError::BadRequest(message.into())
}

pub async fn export(pool: &SqlitePool, event: &EventRow, now: i64) -> Result<Bundle, ApiError> {
    if event.kind != "jeopardy" {
        return Err(ApiError::Conflict(
            "only Jeopardy events have a bundle, tournaments use the tournament backup".into(),
        ));
    }
    let categories = store::categories(pool, event.id).await?;
    let names: HashMap<i64, String> = categories.iter().map(|c| (c.id, c.name.clone())).collect();
    let mut hints = HashMap::<i64, Vec<HintPart>>::new();
    for h in store::hints_of_event(pool, event.id).await? {
        hints.entry(h.challenge_id).or_default().push(HintPart {
            content: h.content,
            cost: h.cost,
            position: h.position,
        });
    }
    let mut challenges = Vec::new();
    for row in store::challenges(pool, event.id).await? {
        let mut attachments = Vec::new();
        for meta in store::attachments_of_challenge(pool, row.id).await? {
            let data = store::attachment_data(pool, meta.id).await?;
            attachments.push(AttachmentPart {
                name: meta.name,
                mime: meta.mime,
                sha256: meta.sha256,
                data_base64: STANDARD.encode(data),
            });
        }
        let fields = ChallengeFields::from(&row);
        challenges.push(ChallengePart {
            category: names.get(&row.category_id).cloned().unwrap_or_default(),
            title: fields.title,
            description: fields.description,
            scoring: fields.scoring,
            flag: fields.flag,
            flag_prefix: fields.flag_prefix,
            flag_suffix: fields.flag_suffix,
            case_sensitive: fields.case_sensitive,
            max_attempts: fields.max_attempts,
            state: row.state.clone(),
            release_at: fields.release_at,
            retire_at: fields.retire_at,
            position: fields.position,
            requires_instance: fields.requires_instance,
            instance_config: fields.instance_config,
            hints: hints.remove(&row.id).unwrap_or_default(),
            attachments,
        });
    }
    Ok(Bundle {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: now,
        event: EventPart {
            name: event.name.clone(),
            description: event.description.clone(),
            mode: event.mode.clone(),
            team_min: event.team_min,
            team_max: event.team_max,
            allow_invites: event.allow_invites,
            roster_locked: event.roster_locked,
            scoreboard_public: event.scoreboard_public,
            network_mode: event.network_mode.clone(),
            registration_opens: event.registration_opens,
            registration_closes: event.registration_closes,
            starts_at: event.starts_at,
            ends_at: event.ends_at,
        },
        settings: jeopardy_store::settings(pool, event.id).await?,
        categories: categories
            .into_iter()
            .map(|c| CategoryPart {
                name: c.name,
                position: c.position,
            })
            .collect(),
        challenges,
    })
}

#[derive(Default, Clone, Copy)]
pub struct ImportOptions {
    /// Keep the registration, start and end times. Off, the new event has no schedule.
    pub keep_times: bool,
    /// Keep published and retired challenges as they were. Off, everything imports as a
    /// draft to be reviewed and published by hand.
    pub keep_state: bool,
    /// Whether this server has a VPN. Without one, a VPN event imports as published.
    pub vpn_available: bool,
}

/// A challenge ready to write: everything decoded and checked before the first insert.
struct Prepared {
    fields: ChallengeFields,
    category: String,
    state: String,
    hints: Vec<(String, i64, i64)>,
    attachments: Vec<(String, String, Vec<u8>)>,
}

fn prepare(bundle: &Bundle) -> Result<(EventFields, Vec<Prepared>), ApiError> {
    if bundle.format != FORMAT || bundle.version != VERSION {
        return Err(bad(format!(
            "this is not a {FORMAT} bundle of version {VERSION}"
        )));
    }
    if bundle.categories.len() > MAX_CATEGORIES || bundle.challenges.len() > MAX_CHALLENGES {
        return Err(bad("the bundle has too many categories or challenges"));
    }
    let name = clean_name(&bundle.event.name, MAX_NAME)?;
    let mode = Mode::from_str(&bundle.event.mode).map_err(|_| bad("unknown event mode"))?;
    let fields = EventFields {
        kind: EventKind::Jeopardy,
        mode,
        name,
        description: bundle.event.description.trim().to_owned(),
        registration_opens: bundle.event.registration_opens,
        registration_closes: bundle.event.registration_closes,
        starts_at: bundle.event.starts_at,
        ends_at: bundle.event.ends_at,
        scoreboard_public: bundle.event.scoreboard_public,
        team_min: bundle.event.team_min,
        team_max: bundle.event.team_max,
        allow_invites: bundle.event.allow_invites,
        roster_locked: bundle.event.roster_locked,
    };
    bundle.settings.validate()?;

    let mut seen = HashSet::new();
    for c in &bundle.categories {
        if c.name.trim().is_empty() || c.name.chars().count() > 60 || !seen.insert(c.name.clone()) {
            return Err(bad("category names are unique, 1 to 60 characters"));
        }
    }
    let mut prepared = Vec::new();
    for part in &bundle.challenges {
        if !seen.contains(&part.category) {
            return Err(bad(format!(
                "challenge {:?} uses an unknown category",
                part.title
            )));
        }
        if !matches!(part.state.as_str(), "draft" | "published" | "retired") {
            return Err(bad("a challenge state is draft, published or retired"));
        }
        let fields = ChallengeFields {
            category_id: 0,
            title: part.title.clone(),
            description: part.description.clone(),
            scoring: part.scoring,
            flag: part.flag.clone(),
            flag_prefix: part.flag_prefix.clone(),
            flag_suffix: part.flag_suffix.clone(),
            case_sensitive: part.case_sensitive,
            max_attempts: part.max_attempts,
            release_at: part.release_at,
            retire_at: part.retire_at,
            position: part.position,
            requires_instance: part.requires_instance,
            instance_config: part.instance_config.clone(),
        };
        fields.validate()?;
        if part.hints.len() > MAX_HINTS || part.attachments.len() > MAX_ATTACHMENTS {
            return Err(bad("a challenge has at most 20 hints and 20 attachments"));
        }
        let mut hints = Vec::new();
        for h in &part.hints {
            hints.push((store::check_hint(&h.content, h.cost)?, h.cost, h.position));
        }
        let mut attachments = Vec::new();
        for a in &part.attachments {
            let data = STANDARD
                .decode(a.data_base64.trim())
                .map_err(|_| bad("an attachment is not valid base64"))?;
            if data.is_empty() || data.len() > MAX_ATTACHMENT {
                return Err(bad("attachments are 1 byte to 10 MB"));
            }
            let digest: String = Sha256::digest(&data)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if !a.sha256.eq_ignore_ascii_case(&digest) {
                return Err(bad(format!(
                    "attachment {:?} does not match its checksum",
                    a.name
                )));
            }
            attachments.push((
                store::clean_filename(&a.name),
                store::clean_mime(&a.mime),
                data,
            ));
        }
        prepared.push(Prepared {
            fields,
            category: part.category.clone(),
            state: part.state.clone(),
            hints,
            attachments,
        });
    }
    Ok((fields, prepared))
}

/// Creates a new event from a bundle. Everything is validated before anything is written;
/// if a write still fails, the half made event is removed again.
pub async fn import(
    pool: &SqlitePool,
    bundle: &Bundle,
    options: ImportOptions,
    now: i64,
) -> Result<EventRow, ApiError> {
    let (mut fields, prepared) = prepare(bundle)?;
    if !options.keep_times {
        fields.registration_opens = None;
        fields.registration_closes = None;
        fields.starts_at = None;
        fields.ends_at = None;
    }
    fields.validate()?;
    let slug = slugify(&fields.name);
    if slug.is_empty() {
        return Err(bad("the event name needs at least one letter or digit"));
    }
    let mut tx = crate::db::begin_write(pool).await?;
    let event_id = event_store::insert(&mut tx, &fields, &slug, now).await?;
    tx.commit().await?;
    match fill(pool, event_id, bundle, prepared, options, now).await {
        Ok(()) => {
            if bundle.event.network_mode == "vpn" && options.vpn_available {
                event_store::set_network_mode(pool, event_id, "vpn").await?;
            }
            event_store::by_id(pool, event_id).await
        }
        Err(e) => {
            let _ = event_store::delete(pool, event_id).await;
            Err(e)
        }
    }
}

async fn fill(
    pool: &SqlitePool,
    event_id: i64,
    bundle: &Bundle,
    prepared: Vec<Prepared>,
    options: ImportOptions,
    now: i64,
) -> Result<(), ApiError> {
    jeopardy_store::save_settings(pool, event_id, &bundle.settings).await?;
    let mut ids = HashMap::new();
    for c in &bundle.categories {
        ids.insert(
            c.name.clone(),
            store::category_insert(pool, event_id, &c.name, c.position).await?,
        );
    }
    for mut item in prepared {
        item.fields.category_id = *ids.get(&item.category).ok_or(ApiError::Internal)?;
        let id = store::challenge_insert(pool, event_id, &item.fields, now).await?;
        for (content, cost, position) in &item.hints {
            store::hint_insert(pool, id, content, *cost, *position).await?;
        }
        for (name, mime, data) in &item.attachments {
            store::attachment_insert(pool, id, name, mime, data).await?;
        }
        if options.keep_state && item.state != "draft" {
            store::set_challenge_state(pool, id, &item.state).await?;
        }
    }
    Ok(())
}
