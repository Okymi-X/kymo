use std::str::FromStr;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use kymo_core::{EventKind, EventState, Mode, can_register};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use crate::auth::{MaybeCaller, Player, Require, needs};
use crate::error::ApiError;
use crate::event_store::{self, EventFields, EventOut, EventRow, MAX_NAME, Participant};
use crate::state::AppState;
use crate::team_store;
use crate::text::{clean_name, slugify};

/// Lets a JSON field be absent (keep), null (clear) or a value (set).
pub(super) fn set_or_clear<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(d)?))
}

fn out(state: &AppState, row: EventRow) -> Result<EventOut, ApiError> {
    EventOut::new(row, state.now())
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<EventOut>>, ApiError> {
    event_store::list(&state.pool, true)
        .await?
        .into_iter()
        .map(|row| out(&state, row))
        .collect::<Result<_, _>>()
        .map(Json)
}

/// The event, plus `registered` when a player session is sent: whether that player (or their
/// team) takes part. Anonymous visitors see `registered: false`.
pub async fn show(
    MaybeCaller(caller): MaybeCaller,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let row = event_store::by_slug(&state.pool, &slug).await?;
    if row.stored_state()? == EventState::Draft {
        return Err(ApiError::NotFound);
    }
    let registered = match caller.and_then(|c| c.user_id) {
        Some(user_id) => crate::jeopardy_store::scorer_of(&state.pool, &row, user_id)
            .await
            .is_ok(),
        None => false,
    };
    let mut value = serde_json::to_value(out(&state, row)?).map_err(|_| ApiError::Internal)?;
    value["registered"] = json!(registered);
    Ok(Json(value))
}

pub async fn admin_list(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
) -> Result<Json<Vec<EventOut>>, ApiError> {
    event_store::list(&state.pool, false)
        .await?
        .into_iter()
        .map(|row| out(&state, row))
        .collect::<Result<_, _>>()
        .map(Json)
}

#[derive(Deserialize)]
pub struct CreateBody {
    name: String,
    kind: String,
    mode: String,
    #[serde(default)]
    description: String,
    registration_opens: Option<i64>,
    registration_closes: Option<i64>,
    starts_at: Option<i64>,
    ends_at: Option<i64>,
    #[serde(default)]
    scoreboard_public: bool,
    team_min: Option<i64>,
    team_max: Option<i64>,
    allow_invites: Option<bool>,
    roster_locked: Option<bool>,
}

fn parse_kind(raw: &str) -> Result<EventKind, ApiError> {
    EventKind::from_str(raw)
        .map_err(|_| ApiError::BadRequest("kind is tournament or jeopardy".into()))
}

fn parse_mode(raw: &str) -> Result<Mode, ApiError> {
    Mode::from_str(raw).map_err(|_| ApiError::BadRequest("mode is solo or team".into()))
}

pub async fn create(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<EventOut>), ApiError> {
    let name = clean_name(&body.name, MAX_NAME)?;
    let slug = slugify(&name);
    if slug.is_empty() {
        return Err(ApiError::BadRequest(
            "name needs at least one letter or digit".into(),
        ));
    }
    let fields = EventFields {
        kind: parse_kind(&body.kind)?,
        mode: parse_mode(&body.mode)?,
        name,
        description: body.description.trim().to_owned(),
        registration_opens: body.registration_opens,
        registration_closes: body.registration_closes,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
        scoreboard_public: body.scoreboard_public,
        team_min: body.team_min.unwrap_or(1),
        team_max: body.team_max.unwrap_or(4),
        allow_invites: body.allow_invites.unwrap_or(true),
        roster_locked: body.roster_locked.unwrap_or(true),
    };
    fields.validate()?;
    let mut tx = state.pool.begin().await?;
    let id = event_store::insert(&mut tx, &fields, &slug, state.now()).await?;
    tx.commit().await?;
    let row = event_store::by_id(&state.pool, id).await?;
    Ok((StatusCode::CREATED, Json(out(&state, row)?)))
}

#[derive(Deserialize)]
pub struct UpdateBody {
    name: Option<String>,
    description: Option<String>,
    mode: Option<String>,
    #[serde(default, deserialize_with = "set_or_clear")]
    registration_opens: Option<Option<i64>>,
    #[serde(default, deserialize_with = "set_or_clear")]
    registration_closes: Option<Option<i64>>,
    #[serde(default, deserialize_with = "set_or_clear")]
    starts_at: Option<Option<i64>>,
    #[serde(default, deserialize_with = "set_or_clear")]
    ends_at: Option<Option<i64>>,
    scoreboard_public: Option<bool>,
    team_min: Option<i64>,
    team_max: Option<i64>,
    allow_invites: Option<bool>,
    roster_locked: Option<bool>,
}

pub async fn update(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<EventOut>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let row = event_store::by_id(&state.pool, id).await?;
    let mut fields = EventFields::from(&row);
    if let Some(name) = &body.name {
        fields.name = clean_name(name, MAX_NAME)?;
    }
    if let Some(description) = body.description {
        fields.description = description.trim().to_owned();
    }
    if let Some(mode) = &body.mode {
        let mode = parse_mode(mode)?;
        if mode != fields.mode && event_store::participant_count(&state.pool, id).await? > 0 {
            return Err(ApiError::Conflict(
                "the mode cannot change once someone has registered".into(),
            ));
        }
        fields.mode = mode;
    }
    fields.registration_opens = body.registration_opens.unwrap_or(fields.registration_opens);
    fields.registration_closes = body
        .registration_closes
        .unwrap_or(fields.registration_closes);
    fields.starts_at = body.starts_at.unwrap_or(fields.starts_at);
    fields.ends_at = body.ends_at.unwrap_or(fields.ends_at);
    fields.scoreboard_public = body.scoreboard_public.unwrap_or(fields.scoreboard_public);
    fields.team_min = body.team_min.unwrap_or(fields.team_min);
    fields.team_max = body.team_max.unwrap_or(fields.team_max);
    fields.allow_invites = body.allow_invites.unwrap_or(fields.allow_invites);
    fields.roster_locked = body.roster_locked.unwrap_or(fields.roster_locked);
    fields.validate()?;
    event_store::update(&state.pool, id, &fields).await?;
    let row = event_store::by_id(&state.pool, id).await?;
    Ok(Json(out(&state, row)?))
}

#[derive(Deserialize)]
pub struct StateBody {
    state: String,
}

pub async fn set_state(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<StateBody>,
) -> Result<Json<EventOut>, ApiError> {
    let next = EventState::from_str(&body.state).map_err(|_| {
        ApiError::BadRequest(
            "state is draft, registration_open, live, paused, ended or archived".into(),
        )
    })?;
    let _guard = state.write_lock.lock().await;
    let row = event_store::by_id(&state.pool, id).await?;
    row.stored_state()?
        .can_move_to(next)
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    event_store::set_state(&state.pool, id, next).await?;
    crate::monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "event_state",
        "event",
        id,
        next.as_str(),
    )
    .await?;
    let row = event_store::by_id(&state.pool, id).await?;
    Ok(Json(out(&state, row)?))
}

pub async fn remove(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.write_lock.lock().await;
    let row = event_store::by_id(&state.pool, id).await?;
    if !matches!(
        row.stored_state()?,
        EventState::Draft | EventState::Archived
    ) {
        return Err(ApiError::Conflict(
            "only a draft or archived event can be deleted".into(),
        ));
    }
    if event_store::has_tournament(&state.pool, id).await? {
        return Err(ApiError::Conflict(
            "this event belongs to a tournament, delete the tournament instead".into(),
        ));
    }
    event_store::delete(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn participants(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Participant>>, ApiError> {
    event_store::by_id(&state.pool, id).await?;
    Ok(Json(event_store::participants(&state.pool, id).await?))
}

/// Who registers: the player for a solo event, the player's team (owner only) for a team event.
async fn registrant(
    state: &AppState,
    player: &Player,
    row: &EventRow,
) -> Result<(Option<i64>, Option<i64>), ApiError> {
    match row.mode()? {
        Mode::Solo => Ok((Some(player.id), None)),
        Mode::Team => {
            let team = team_store::of_user(&state.pool, player.id)
                .await?
                .ok_or_else(|| {
                    ApiError::Conflict("create or join a team before registering".into())
                })?;
            if team.owner_user_id != player.id {
                return Err(ApiError::Conflict(
                    "only the team owner can register or withdraw the team".into(),
                ));
            }
            Ok((None, Some(team.id)))
        }
    }
}

fn check_window(state: &AppState, row: &EventRow) -> Result<(), ApiError> {
    can_register(row.stored_state()?, &row.schedule(), state.now())
        .map_err(|denied| ApiError::Conflict(denied.message().into()))
}

pub async fn register(
    player: Player,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let _guard = state.write_lock.lock().await;
    let row = event_store::by_slug(&state.pool, &slug).await?;
    check_window(&state, &row)?;
    let (user_id, team_id) = registrant(&state, &player, &row).await?;
    if let Some(team_id) = team_id {
        let rules = row.team_rules()?;
        if !rules.is_valid_size(team_store::size(&state.pool, team_id).await?) {
            return Err(ApiError::Conflict(format!(
                "this event needs teams of {} to {} members",
                rules.min_size, rules.max_size
            )));
        }
    }
    event_store::register(&state.pool, row.id, user_id, team_id, state.now()).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "event_id": row.id, "registered": true })),
    ))
}

/// Withdrawing is allowed while registering is allowed; after that the roster is frozen.
pub async fn unregister(
    player: Player,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.write_lock.lock().await;
    let row = event_store::by_slug(&state.pool, &slug).await?;
    check_window(&state, &row)?;
    let (user_id, team_id) = registrant(&state, &player, &row).await?;
    event_store::unregister(&state.pool, row.id, user_id, team_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
