//! Instance endpoints: players run their own container for a challenge, admins see and stop
//! any run. Rules and ordering live in `crate::instances`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::auth::{Player, Require, needs};
use crate::error::ApiError;
use crate::event_store;
use crate::instance_store;
use crate::instances;
use crate::state::AppState;
use crate::submit::{challenge_of, play_context, released};

async fn context(
    state: &AppState,
    player: &Player,
    slug: &str,
    challenge_id: i64,
) -> Result<(crate::submit::Play, crate::challenge_store::ChallengeRow), ApiError> {
    let play = play_context(state, slug, player.id).await?;
    let challenge = challenge_of(state, &play, challenge_id).await?;
    released(&challenge, state.now())?;
    Ok((play, challenge))
}

pub async fn show(
    player: Player,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id).await?;
    Ok(Json(instances::current(&state, &play, &challenge).await?))
}

pub async fn start(
    player: Player,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id).await?;
    let view = instances::start(&state, &play, &challenge, player.id).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

pub async fn restart(
    player: Player,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id).await?;
    Ok(Json(
        instances::restart(&state, &play, &challenge, player.id).await?,
    ))
}

pub async fn stop(
    player: Player,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let (play, challenge) = context(&state, &player, &slug, id).await?;
    Ok(Json(instances::stop(&state, &play, &challenge).await?))
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
    player: Player,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let play = play_context(&state, &slug, player.id).await?;
    Ok(Json(instances::list_for(&state, &play).await?))
}
