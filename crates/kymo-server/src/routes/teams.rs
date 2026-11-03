use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use kymo_core::{EventState, TeamError, TeamRules, can_invite, can_leave};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::Player;
use crate::error::ApiError;
use crate::event_store::{self, EventRow};
use crate::state::AppState;
use crate::team_store::{self, TeamOut, TeamRow};
use crate::text::clean_name;

const MAX_TEAM_NAME: usize = 32;

type RosterRule = fn(&TeamRules, u32, EventState) -> Result<(), TeamError>;

/// Applies a roster rule from kymo-core against every event the team is registered in.
async fn roster_allows(state: &AppState, team_id: i64, rule: RosterRule) -> Result<(), ApiError> {
    let events: Vec<EventRow> = event_store::of_team(&state.pool, team_id).await?;
    if events.is_empty() {
        return Ok(());
    }
    let size = team_store::size(&state.pool, team_id).await?;
    let now = state.now();
    for event in events {
        rule(&event.team_rules()?, size, event.effective_state(now)?)
            .map_err(|e| ApiError::Conflict(roster_message(e, &event.name)))?;
    }
    Ok(())
}

/// What a person should read when a roster change is refused: the event by name, and why.
fn roster_message(error: TeamError, event: &str) -> String {
    match error {
        TeamError::RosterLocked => format!(
            "{event} has started and its team rosters are locked, so members cannot join or leave. An organizer can unlock rosters for the event."
        ),
        TeamError::EventClosed => {
            format!("{event} has ended, so its team rosters can no longer change.")
        }
        TeamError::Full => format!("The team is full: {event} allows no more members than it has."),
        TeamError::InvitesDisabled => {
            format!("Joining with invite codes is turned off, invites are disabled for {event}.")
        }
        TeamError::Empty => format!("The team has no members to remove for {event}."),
    }
}

/// A team the caller belongs to. Anyone else gets 404, so team ids reveal nothing.
async fn member_team(state: &AppState, player: &Player, id: i64) -> Result<TeamRow, ApiError> {
    if !team_store::is_member(&state.pool, id, player.id).await? {
        return Err(ApiError::NotFound);
    }
    team_store::by_id(&state.pool, id).await
}

#[derive(Deserialize)]
pub struct CreateBody {
    name: String,
}

pub async fn create(
    player: Player,
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<TeamOut>), ApiError> {
    let name = clean_name(&body.name, MAX_TEAM_NAME)?;
    let _guard = state.write_lock.lock().await;
    if team_store::of_user(&state.pool, player.id).await?.is_some() {
        return Err(ApiError::Conflict("leave your current team first".into()));
    }
    let team = team_store::create(&state.pool, &name, player.id, state.now()).await?;
    let view = team_store::view(&state.pool, team, player.id).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

pub async fn mine(
    player: Player,
    State(state): State<AppState>,
) -> Result<Json<TeamOut>, ApiError> {
    let team = team_store::of_user(&state.pool, player.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(team_store::view(&state.pool, team, player.id).await?))
}

pub async fn show(
    player: Player,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TeamOut>, ApiError> {
    let team = member_team(&state, &player, id).await?;
    Ok(Json(team_store::view(&state.pool, team, player.id).await?))
}

#[derive(Deserialize)]
pub struct JoinBody {
    invite_code: String,
}

pub async fn join(
    player: Player,
    State(state): State<AppState>,
    Json(body): Json<JoinBody>,
) -> Result<Json<TeamOut>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let team = team_store::by_code(&state.pool, body.invite_code.trim()).await?;
    if team_store::of_user(&state.pool, player.id).await?.is_some() {
        return Err(ApiError::Conflict("leave your current team first".into()));
    }
    roster_allows(&state, team.id, can_invite).await?;
    team_store::add_member(&state.pool, team.id, player.id, state.now()).await?;
    // A new member may now reach the team's lab.
    crate::vpn::sync_quiet(&state).await;
    Ok(Json(team_store::view(&state.pool, team, player.id).await?))
}

pub async fn leave(
    player: Player,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.write_lock.lock().await;
    member_team(&state, &player, id).await?;
    roster_allows(&state, id, can_leave).await?;
    team_store::remove_member(&state.pool, id, player.id).await?;
    crate::vpn::sync_quiet(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rotate_code(
    player: Player,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let team = member_team(&state, &player, id).await?;
    if team.owner_user_id != player.id {
        return Err(ApiError::Denied("only the team owner can do that".into()));
    }
    let code = team_store::rotate_code(&state.pool, id).await?;
    Ok(Json(json!({ "invite_code": code })))
}

pub async fn kick(
    player: Player,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.write_lock.lock().await;
    let team = member_team(&state, &player, id).await?;
    if team.owner_user_id != player.id {
        return Err(ApiError::Denied("only the team owner can do that".into()));
    }
    if user_id == player.id {
        return Err(ApiError::Conflict(
            "use leave to leave your own team".into(),
        ));
    }
    if !team_store::is_member(&state.pool, id, user_id).await? {
        return Err(ApiError::NotFound);
    }
    roster_allows(&state, id, can_leave).await?;
    team_store::remove_member(&state.pool, id, user_id).await?;
    crate::vpn::sync_quiet(&state).await;
    Ok(StatusCode::NO_CONTENT)
}
