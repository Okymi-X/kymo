use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use kymo_engine::{Format, Options, Side, generate_with};
use rand::seq::SliceRandom;
use serde::Deserialize;

use crate::auth::Admin;
use crate::db::{self, NewTeam, Status, Tournament};
use crate::error::ApiError;
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
pub struct ResultBody {
    score_a: u32,
    score_b: u32,
}

/// Invisible or direction changing characters let a name look like another one.
fn is_hidden_char(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

fn clean_name(raw: &str, max: usize) -> Result<String, ApiError> {
    if raw.chars().any(is_hidden_char) {
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
    let id = db::create(
        &state.pool,
        &name,
        &slug,
        body.format,
        body.third_place && body.format == Format::SingleElimination,
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
    if body.teams.len() > kymo_engine::MAX_TEAMS {
        return Err(ApiError::BadRequest(format!(
            "at most {} teams",
            kymo_engine::MAX_TEAMS
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
        },
    )?;
    db::save_bracket(&state.pool, id, Some(&bracket), Status::Live).await?;
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
    state.notify(&t.slug);
    Ok(Json(db::get(&state.pool, id).await?))
}

async fn edit_bracket(
    state: &AppState,
    id: i64,
    edit: impl FnOnce(&mut kymo_engine::Bracket) -> Result<(), kymo_engine::EngineError>,
) -> Result<Tournament, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    let mut bracket = t
        .bracket
        .ok_or_else(|| ApiError::Conflict("tournament has not started".into()))?;
    edit(&mut bracket)?;
    let status = if bracket.champion().is_some() {
        Status::Done
    } else {
        Status::Live
    };
    db::save_bracket(&state.pool, id, Some(&bracket), status).await?;
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
    let t = edit_bracket(&state, id, |b| {
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
        edit_bracket(&state, id, |b| b.set_forfeit(match_id, body.winner)).await?,
    ))
}

pub async fn clear_result(
    _: Admin,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
) -> Result<Json<Tournament>, ApiError> {
    Ok(Json(
        edit_bracket(&state, id, |b| b.clear_result(match_id)).await?,
    ))
}

#[derive(Deserialize)]
pub struct TokenBody {
    label: String,
}

pub async fn list_tokens(
    _: Admin,
    State(state): State<AppState>,
) -> Result<Json<Vec<ApiToken>>, ApiError> {
    Ok(Json(tokens::list_api(&state.pool).await?))
}

/// The secret appears only in this response; the server keeps its hash.
pub async fn create_token(
    _: Admin,
    State(state): State<AppState>,
    Json(body): Json<TokenBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let label = clean_name(&body.label, MAX_NAME)?;
    let secret = tokens::issue(&state.pool, Kind::Api, &label).await?;
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
