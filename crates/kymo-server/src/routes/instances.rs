//! Instance endpoints: players run their own container for a challenge, admins see and stop
//! any run. Rules and ordering live in `crate::instances`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::auth::{Account, ClientIp, Player, Require, needs};
use crate::error::ApiError;
use crate::event_store;
use crate::instance_store;
use crate::instances;
use crate::state::AppState;
use crate::submit::{challenge_of, play_context_any, released};

/// The player's current instance in any event, for the header. None when they have none.
pub async fn mine(
    Account(player, _): Account<needs::StartInstance>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(instances::mine(&state, player.id).await?))
}

async fn context(
    state: &AppState,
    player: &Player,
    slug: &str,
    challenge_id: i64,
    starting: bool,
) -> Result<(crate::submit::Play, crate::challenge_store::ChallengeRow), ApiError> {
    let play = play_context_any(state, slug, player.id).await?;
    let challenge = challenge_of(state, &play, challenge_id).await?;
    released(&challenge, state.now())?;
    // In a tournament only the players of a match the challenge is assigned to may use it.
    crate::duel::player_gate(state, &play.event, challenge.id, player.id, starting).await?;
    Ok((play, challenge))
}

/// Writes who started, restarted or stopped what, from which address, whether or not it worked.
/// A failure here never fails the player's action: the record is for the organizer.
async fn record(
    state: &AppState,
    player: &Player,
    ip: &ClientIp,
    action: &str,
    slug: &str,
    challenge: &crate::challenge_store::ChallengeRow,
    outcome: &Result<Value, ApiError>,
) {
    // Where the run lives is not in the player's view; the organizer wants it in the record.
    let lab_host = match outcome.as_ref().ok().and_then(|v| v["id"].as_i64()) {
        Some(id) => instance_store::by_id(&state.pool, id)
            .await
            .ok()
            .and_then(|r| r.lab_host_id)
            .map_or("local".to_owned(), |l| l.to_string()),
        None => "-".to_owned(),
    };
    let (action, detail) = match outcome {
        Ok(view) => (
            action.to_owned(),
            format!(
                "event={slug} challenge={} \"{}\" run={} lab_host={} address={}:{}",
                challenge.id,
                challenge.title,
                view["id"],
                lab_host,
                view["host"].as_str().unwrap_or("-"),
                view["port"]
                    .as_i64()
                    .map_or("-".to_owned(), |p| p.to_string()),
            ),
        ),
        Err(e) => (
            format!("{action}_failed"),
            format!(
                "event={slug} challenge={} \"{}\" error={e:?}",
                challenge.id, challenge.title
            ),
        ),
    };
    let _ = crate::monitor_store::audit_from(
        &state.pool,
        state.now(),
        Some(player.id),
        "player",
        Some(&ip.0.to_string()),
        &action,
        "challenge",
        challenge.id,
        &detail,
    )
    .await;
}

pub async fn show(
    Account(player, _): Account<needs::PlayEvent>,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id, false).await?;
    Ok(Json(instances::current(&state, &play, &challenge).await?))
}

pub async fn start(
    Account(player, _): Account<needs::StartInstance>,
    ip: ClientIp,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id, true).await?;
    let outcome = instances::start(&state, &play, &challenge, player.id).await;
    record(
        &state,
        &player,
        &ip,
        "instance_start",
        &slug,
        &challenge,
        &outcome,
    )
    .await;
    Ok((StatusCode::CREATED, Json(outcome?)))
}

pub async fn restart(
    Account(player, _): Account<needs::StartInstance>,
    ip: ClientIp,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id, true).await?;
    let outcome = instances::restart(&state, &play, &challenge, player.id).await;
    record(
        &state,
        &player,
        &ip,
        "instance_restart",
        &slug,
        &challenge,
        &outcome,
    )
    .await;
    Ok(Json(outcome?))
}

pub async fn stop(
    Account(player, _): Account<needs::PlayEvent>,
    ip: ClientIp,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id, false).await?;
    let outcome = instances::stop(&state, &play, &challenge).await;
    record(
        &state,
        &player,
        &ip,
        "instance_stop",
        &slug,
        &challenge,
        &outcome,
    )
    .await;
    Ok(Json(outcome?))
}

pub async fn admin_list(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(event_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    event_store::by_id(&state.pool, event_id).await?;
    let rows = instance_store::list_for_event(&state.pool, event_id).await?;
    let now = state.now();
    let mut list: Vec<Value> = Vec::with_capacity(rows.len());
    for r in &rows {
        let state_name = r
            .record()
            .map(|rec| kymo_instances::effective_state(&rec, now).as_str())
            .unwrap_or("failed");
        let username: Option<String> = sqlx::query_scalar("SELECT username FROM user WHERE id = ?")
            .bind(r.user_id)
            .fetch_optional(&state.pool)
            .await?;
        let team: Option<String> = if r.scorer_kind == "team" {
            sqlx::query_scalar("SELECT name FROM player_team WHERE id = ?")
                .bind(r.scorer_id)
                .fetch_optional(&state.pool)
                .await?
        } else {
            None
        };
        let title: Option<String> = sqlx::query_scalar("SELECT title FROM challenge WHERE id = ?")
            .bind(r.challenge_id)
            .fetch_optional(&state.pool)
            .await?;
        list.push(json!({
            "id": r.id,
            "challenge_id": r.challenge_id,
            "challenge_title": title,
            "participant": { "kind": r.scorer_kind, "id": r.scorer_id, "team": team },
            "user_id": r.user_id,
            "username": username,
            "state": state_name,
            "host_port": r.host_port,
            "started_at": r.started_at,
            "expires_at": r.expires_at,
            "stopped_at": r.stopped_at,
        }));
    }
    Ok(Json(json!({ "instances": list })))
}

pub async fn admin_stop(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = instances::force_stop(&state, id).await?;
    crate::monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "instance_stop",
        "instance",
        id,
        "",
    )
    .await?;
    Ok(Json(json!({ "id": row.id, "state": row.state })))
}

pub async fn admin_reap(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "ended": instances::reap(&state).await? })))
}

pub async fn list(
    Account(player, _): Account<needs::PlayEvent>,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let play = crate::submit::play_context(&state, &slug, player.id).await?;
    Ok(Json(instances::list_for(&state, &play).await?))
}

/// The flags generated for one run, for an organizer checking a machine by hand. Reading them is
/// audited, like revealing a submitted value.
pub async fn admin_flags(
    who: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = instance_store::by_id(&state.pool, id).await?;
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT f.name, f.label, i.value FROM instance_flag i JOIN challenge_flag f ON f.id = i.flag_id WHERE i.instance_id = ? ORDER BY f.position, f.id",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let mut flags: Vec<Value> = rows
        .into_iter()
        .map(|(name, label, value)| json!({ "name": name, "label": label, "value": value }))
        .collect();
    // A static Responder holds the challenge's own flag.
    let challenge = crate::challenge_store::challenge_by_id(&state.pool, row.challenge_id).await?;
    if flags.is_empty()
        && challenge.responder
        && let kymo_jeopardy::FlagSpec::Static { value } = &challenge.flag
    {
        flags.push(json!({ "name": "flag", "label": "", "value": value }));
    }
    crate::monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "instance_flags_viewed",
        "instance",
        id,
        "",
    )
    .await?;
    Ok(Json(json!({ "flags": flags })))
}

/// What the Verifier of a run recorded: which check ran, when, and what it said. For organizers
/// only: players never see check output.
pub async fn admin_verify_log(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = instance_store::by_id(&state.pool, id).await?;
    let (Some(host), Some(handle)) = (row.lab_host_id, row.handle.clone()) else {
        return Err(ApiError::NotFound);
    };
    let lab = crate::lab_store::get(&state.pool, host).await?;
    let client = state
        .labs()
        .client(&lab)
        .map_err(|_| ApiError::Unavailable("the lab host cannot be reached".into()))?;
    let log = client
        .verify_log(&handle)
        .await
        .map_err(|_| ApiError::Unavailable("the Verifier does not answer".into()))?;
    Ok(Json(json!({ "log": log })))
}
