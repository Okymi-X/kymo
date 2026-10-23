use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use kymo_engine::{Format, Options, Side, generate_with};
use rand::seq::SliceRandom;
use serde::Deserialize;

use crate::auth::{Admin, SessionAdmin};
use crate::db::{self, NewTeam, NewTournament, Status, Tournament};
use crate::error::ApiError;
use crate::events;
use crate::settings::{self, Appearance};
use crate::state::AppState;
use crate::tokens::{self, ApiToken, Kind};

const MAX_NAME: usize = 80;
const MAX_TEAM_NAME: usize = 64;
const MAX_SCORE: u32 = 1_000_000;

#[derive(Deserialize)]
pub struct CreateBody {
    name: String,
    format: Format,
    #[serde(default)]
    third_place: bool,
    #[serde(default = "default_groups")]
    groups: u8,
    #[serde(default = "default_advance")]
    advance: u8,
}

fn default_groups() -> u8 {
    2
}

fn default_advance() -> u8 {
    2
}

#[derive(Deserialize)]
pub struct NameBody {
    name: String,
}

#[derive(Deserialize)]
pub struct TeamsBody {
    teams: Vec<TeamInput>,
}

#[derive(Deserialize)]
pub struct TeamInput {
    name: String,
    #[serde(default)]
    score: u32,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Seeding {
    #[default]
    Manual,
    Random,
    Score,
}

#[derive(Deserialize, Default)]
pub struct StartBody {
    #[serde(default)]
    seeding: Seeding,
}

#[derive(Deserialize)]
pub struct ForfeitBody {
    winner: Side,
}

#[derive(Deserialize)]
pub struct ScheduleBody {
    at: Option<i64>,
    venue: Option<String>,
}

#[derive(Deserialize)]
pub struct ResultBody {
    score_a: u32,
    score_b: u32,
}

fn clean_name(raw: &str, max: usize) -> Result<String, ApiError> {
    if raw.chars().any(crate::text::is_hidden_char) {
        return Err(ApiError::BadRequest(
            "names cannot contain control or invisible characters".into(),
        ));
    }
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > max {
        return Err(ApiError::BadRequest(format!(
            "name must be 1 to {max} characters"
        )));
    }
    Ok(name.to_owned())
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

pub async fn create(
    _: Admin,
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<Tournament>), ApiError> {
    let name = clean_name(&body.name, MAX_NAME)?;
    let slug = slugify(&name);
    if slug.is_empty() {
        return Err(ApiError::BadRequest(
            "name needs at least one letter or digit".into(),
        ));
    }
    if body.format == Format::GroupKnockout
        && (!(2..=8).contains(&body.groups) || !(1..=4).contains(&body.advance))
    {
        return Err(ApiError::BadRequest(
            "use 2 to 8 groups and 1 to 4 teams advancing per group".into(),
        ));
    }
    let id = db::create(
        &state.pool,
        NewTournament {
            name: &name,
            slug: &slug,
            format: body.format,
            third_place: body.third_place && body.format == Format::SingleElimination,
            groups: body.groups,
            advance: body.advance,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(db::get(&state.pool, id).await?)))
}

pub async fn rename(
    _: Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<NameBody>,
) -> Result<Json<Tournament>, ApiError> {
    db::rename(&state.pool, id, &clean_name(&body.name, MAX_NAME)?).await?;
    let t = db::get(&state.pool, id).await?;
    state.notify(&t.slug);
    Ok(Json(t))
}

pub async fn remove(
    _: Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let t = db::get(&state.pool, id).await?;
    db::delete(&state.pool, id).await?;
    state.notify(&t.slug);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_teams(
    _: Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<TeamsBody>,
) -> Result<Json<Tournament>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    if t.status != Status::Draft {
        return Err(ApiError::Conflict(
            "teams can only change before the tournament starts".into(),
        ));
    }
    let max_teams = if matches!(t.format, Format::RoundRobin | Format::GroupKnockout) {
        kymo_engine::MAX_ROUND_ROBIN_TEAMS
    } else {
        kymo_engine::MAX_TEAMS
    };
    if body.teams.len() > max_teams {
        return Err(ApiError::BadRequest(format!(
            "at most {max_teams} teams for this format"
        )));
    }
    let teams = body
        .teams
        .iter()
        .map(|t| {
            if t.score > MAX_SCORE {
                return Err(ApiError::BadRequest(format!(
                    "qualifier scores must be at most {MAX_SCORE}"
                )));
            }
            Ok(NewTeam {
                name: clean_name(&t.name, MAX_TEAM_NAME)?,
                score: i64::from(t.score),
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let mut seen = HashSet::new();
    if let Some(dup) = teams.iter().find(|t| !seen.insert(t.name.to_lowercase())) {
        return Err(ApiError::BadRequest(format!(
            "duplicate team: {}",
            dup.name
        )));
    }
    db::replace_teams(&state.pool, id, &teams).await?;
    state.notify(&t.slug);
    Ok(Json(db::get(&state.pool, id).await?))
}

pub async fn start(
    _: Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<StartBody>>,
) -> Result<Json<Tournament>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    if t.status != Status::Draft {
        return Err(ApiError::Conflict("tournament already started".into()));
    }
    let mut teams = t.teams.clone();
    match body.map(|b| b.0.seeding).unwrap_or_default() {
        Seeding::Manual => {}
        Seeding::Random => teams.shuffle(&mut rand::rng()),
        Seeding::Score => teams.sort_by_key(|team| std::cmp::Reverse(team.score)),
    }
    let reseeded: Vec<NewTeam> = teams
        .iter()
        .map(|team| NewTeam {
            name: team.name.clone(),
            score: team.score,
        })
        .collect();
    db::replace_teams(&state.pool, id, &reseeded).await?;
    teams = db::get(&state.pool, id).await?.teams;
    let ids: Vec<u32> = teams.iter().map(|team| team.id as u32).collect();
    let bracket = generate_with(
        &ids,
        t.format,
        Options {
            third_place: t.third_place,
            groups: t.groups,
            advance: t.advance,
        },
    )?;
    db::save_bracket(&state.pool, id, Some(&bracket), Status::Live).await?;
    events::record_tournament(
        &state.pool,
        id,
        "started",
        serde_json::json!({ "teams": ids.len(), "format": t.format }),
    )
    .await?;
    state.notify(&t.slug);
    Ok(Json(db::get(&state.pool, id).await?))
}

pub async fn reset(
    _: Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Tournament>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    db::save_bracket(&state.pool, id, None, Status::Draft).await?;
    events::record_tournament(&state.pool, id, "reset", serde_json::json!({})).await?;
    state.notify(&t.slug);
    Ok(Json(db::get(&state.pool, id).await?))
}

/// Applies one change to a match under the write lock and records it in the activity log.
/// A change to a match that already had a result is logged as a correction.
async fn edit_bracket(
    state: &AppState,
    id: i64,
    match_id: usize,
    kind: &'static str,
    edit: impl FnOnce(&mut kymo_engine::Bracket) -> Result<(), kymo_engine::EngineError>,
) -> Result<Tournament, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    let mut bracket = t
        .bracket
        .ok_or_else(|| ApiError::Conflict("tournament has not started".into()))?;
    let had_result = bracket.matches.get(match_id).is_some_and(|m| m.is_played());
    edit(&mut bracket)?;
    let status = if bracket.champion().is_some() {
        Status::Done
    } else {
        Status::Live
    };
    db::save_bracket(&state.pool, id, Some(&bracket), status).await?;
    let kind = if had_result && kind != "cleared" {
        "corrected"
    } else {
        kind
    };
    events::record_match(&state.pool, id, kind, &bracket, match_id).await?;
    state.notify(&t.slug);
    db::get(&state.pool, id).await
}

pub async fn set_result(
    _: Admin,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
    Json(body): Json<ResultBody>,
) -> Result<Json<Tournament>, ApiError> {
    if body.score_a > MAX_SCORE || body.score_b > MAX_SCORE {
        return Err(ApiError::BadRequest(format!(
            "scores must be at most {MAX_SCORE}"
        )));
    }
    let t = edit_bracket(&state, id, match_id, "result", |b| {
        b.set_result(match_id, body.score_a, body.score_b)
    })
    .await?;
    Ok(Json(t))
}

pub async fn set_forfeit(
    _: Admin,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
    Json(body): Json<ForfeitBody>,
) -> Result<Json<Tournament>, ApiError> {
    Ok(Json(
        edit_bracket(&state, id, match_id, "forfeit", |b| {
            b.set_forfeit(match_id, body.winner)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct TiebreakBody {
    order: Vec<u32>,
}

/// Decides the order of teams that tie, for tables and for who qualifies from a group.
pub async fn set_tiebreak(
    _: Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<TiebreakBody>,
) -> Result<Json<Tournament>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    let mut bracket = t
        .bracket
        .ok_or_else(|| ApiError::Conflict("tournament has not started".into()))?;
    bracket.set_tiebreak(body.order)?;
    let status = if bracket.champion().is_some() {
        Status::Done
    } else {
        Status::Live
    };
    db::save_bracket(&state.pool, id, Some(&bracket), status).await?;
    state.notify(&t.slug);
    Ok(Json(db::get(&state.pool, id).await?))
}

pub async fn clear_result(
    _: Admin,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
) -> Result<Json<Tournament>, ApiError> {
    Ok(Json(
        edit_bracket(&state, id, match_id, "cleared", |b| {
            b.clear_result(match_id)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct TokenBody {
    label: String,
    /// Days until the token stops working, 1 to 365. Missing means it never expires.
    #[serde(default)]
    expires_days: Option<u32>,
}

pub async fn list_tokens(
    _: Admin,
    State(state): State<AppState>,
) -> Result<Json<Vec<ApiToken>>, ApiError> {
    Ok(Json(tokens::list_api(&state.pool).await?))
}

/// The secret appears only in this response; the server keeps its hash.
pub async fn create_token(
    _: SessionAdmin,
    State(state): State<AppState>,
    Json(body): Json<TokenBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let label = clean_name(&body.label, MAX_NAME)?;
    if body.expires_days.is_some_and(|d| !(1..=365).contains(&d)) {
        return Err(ApiError::BadRequest("expiry must be 1 to 365 days".into()));
    }
    let secret = tokens::issue(&state.pool, Kind::Api, &label, body.expires_days).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "label": label, "token": secret })),
    ))
}

pub async fn revoke_token(
    _: Admin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    tokens::revoke_api(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn save_appearance(
    _: Admin,
    State(state): State<AppState>,
    Json(mut body): Json<Appearance>,
) -> Result<Json<Appearance>, ApiError> {
    body.validate()?;
    settings::save(&state.pool, &body).await?;
    Ok(Json(body))
}

pub async fn reset_appearance(
    _: Admin,
    State(state): State<AppState>,
) -> Result<Json<Appearance>, ApiError> {
    settings::reset(&state.pool).await?;
    Ok(Json(Appearance::default()))
}

/// Adds the sample tournaments that are missing. Safe to call repeatedly.
pub async fn load_demo(
    _: Admin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let added = crate::demo::seed(&state.pool).await?;
    Ok(Json(serde_json::json!({ "added": added })))
}

const MAX_VENUE: usize = 40;
/// 2000-01-01 and 2100-01-01 in unix seconds: anything else is a typo, not a plan.
const EARLIEST: i64 = 946_684_800;
const LATEST: i64 = 4_102_444_800;

pub async fn set_schedule(
    _: Admin,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
    Json(body): Json<ScheduleBody>,
) -> Result<Json<Tournament>, ApiError> {
    if body.at.is_some_and(|at| !(EARLIEST..=LATEST).contains(&at)) {
        return Err(ApiError::BadRequest("the time is out of range".into()));
    }
    let venue = match body
        .venue
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(v) if v.chars().count() > MAX_VENUE || v.chars().any(crate::text::is_hidden_char) => {
            return Err(ApiError::BadRequest(format!(
                "the place must be at most {MAX_VENUE} characters without control characters"
            )));
        }
        other => other.map(str::to_owned),
    };
    let at = body.at;
    Ok(Json(
        edit_bracket(&state, id, match_id, "scheduled", |b| {
            b.set_schedule(match_id, at, venue)
        })
        .await?,
    ))
}
