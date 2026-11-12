//! The public face of the platform: the home page content, an event's rules, and the short
//! summary shown above a public scoreboard.

use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};

use crate::auth::{Admin, Require, needs};
use crate::error::ApiError;
use crate::event_store;
use crate::home::{self, Home};
use crate::rules::{self, Rules};
use crate::state::AppState;

/// What the home page shows. Open to everyone: it is the page a visitor sees before signing in.
pub async fn home(State(state): State<AppState>) -> Result<Json<Home>, ApiError> {
    Ok(Json(home::load(&state.pool).await?))
}

pub async fn save_home(
    _: Admin,
    State(state): State<AppState>,
    Json(mut body): Json<Home>,
) -> Result<Json<Home>, ApiError> {
    body.validate()?;
    home::save(&state.pool, &body).await?;
    crate::monitor_store::audit(
        &state.pool,
        state.now(),
        None,
        "home.update",
        "setting",
        "home",
        if body.mode == home::Mode::Custom {
            "custom page"
        } else {
            "default page"
        },
    )
    .await?;
    Ok(Json(body))
}

pub async fn reset_home(_: Admin, State(state): State<AppState>) -> Result<Json<Home>, ApiError> {
    home::reset(&state.pool).await?;
    Ok(Json(Home::default()))
}

/// The rules of an event, for a signed in player. Empty text when it has none.
pub async fn rules(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Rules>, ApiError> {
    let event = event_store::by_slug(&state.pool, &slug).await?;
    Ok(Json(rules::get(&state.pool, event.id).await?))
}

pub async fn save_rules(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(mut body): Json<Rules>,
) -> Result<Json<Rules>, ApiError> {
    let event = event_store::by_id(&state.pool, id).await?;
    body.validate()?;
    rules::put(&state.pool, event.id, &body, state.now()).await?;
    Ok(Json(body))
}

/// The events whose scoreboard is public, for the home page: name, kind and state only.
pub async fn public_scoreboards(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let now = state.now();
    let mut out = Vec::new();
    for row in event_store::list(&state.pool, true).await? {
        if !row.scoreboard_public {
            continue;
        }
        let e = event_store::EventOut::new(row, now)?;
        out.push(json!({
            "name": e.name,
            "slug": e.slug,
            "kind": e.kind,
            "effective_state": e.effective_state,
            "starts_at": e.starts_at,
            "ends_at": e.ends_at,
        }));
    }
    Ok(Json(json!(out)))
}

/// The short summary shown above a public scoreboard. The sign in rule only lets anonymous
/// visitors reach it for an event whose scoreboard is public; a signed in visitor gets it for
/// any event they can see, and nothing in it goes beyond what the event list already shows.
pub async fn public_event(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let row = event_store::by_slug(&state.pool, &slug).await?;
    let out = event_store::EventOut::new(row, state.now())?;
    Ok(Json(json!({
        "name": out.name,
        "slug": out.slug,
        "kind": out.kind,
        "mode": out.mode,
        "effective_state": out.effective_state,
        "starts_at": out.starts_at,
        "ends_at": out.ends_at,
        "scoreboard_public": out.scoreboard_public,
    })))
}
