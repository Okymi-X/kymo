use std::convert::Infallible;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::ClientIp;
use crate::db::{self, Tournament};
use crate::error::ApiError;
use crate::events::{self, Event as LogEvent};
use crate::settings::{self, Appearance};
use crate::state::AppState;
use crate::tokens::{self, Kind};

#[derive(Deserialize)]
pub struct LoginBody {
    password: String,
}

#[derive(Serialize)]
pub struct LoginOut {
    token: String,
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(client): ClientIp,
    Json(body): Json<LoginBody>,
) -> Result<Json<LoginOut>, ApiError> {
    if state.login_blocked(client) {
        return Err(ApiError::TooManyRequests);
    }
    let ok = state.verify_password(body.password).await;
    state.record_login(client, ok);
    if !ok {
        return Err(ApiError::Unauthorized);
    }
    let token = tokens::issue(&state.pool, Kind::Session, "", None).await?;
    Ok(Json(LoginOut { token }))
}

/// Ends the caller's session on the server. Idempotent: unknown tokens still get 204.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if let Some(secret) = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        tokens::end_session(&state.pool, secret).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
pub struct ReportBody {
    /// The reporting team's score.
    own: u32,
    /// The opponent's score.
    other: u32,
}

/// A captain opens their private link to see which match to report.
pub async fn report_info(
    State(state): State<AppState>,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> Result<Json<crate::reports::Assignment>, ApiError> {
    Ok(Json(crate::reports::assignment(&state.pool, &code).await?))
}

/// A captain submits the score. It waits for the organizer and changes nothing until approved.
pub async fn report_submit(
    State(state): State<AppState>,
    axum::extract::Path(code): axum::extract::Path<String>,
    Json(body): Json<ReportBody>,
) -> Result<StatusCode, ApiError> {
    crate::reports::submit(&state.pool, &code, body.own, body.other).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Tells a signed in caller what its token is: the organizer, or a referee for some tournaments.
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let secret = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    let access = tokens::access(&state.pool, secret)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    Ok(Json(if access.referee {
        serde_json::json!({ "role": "referee", "tournaments": access.scope })
    } else {
        serde_json::json!({ "role": "organizer", "tournaments": null })
    }))
}

/// The list only needs a few names for the avatar stack, not every roster.
const LIST_TEAMS: usize = 4;

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Tournament>>, ApiError> {
    let mut all = db::list(&state.pool).await?;
    for t in &mut all {
        t.bracket = None;
        t.teams.truncate(LIST_TEAMS);
    }
    Ok(Json(all))
}

pub async fn show(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Tournament>, ApiError> {
    Ok(Json(db::get_by_slug(&state.pool, &slug).await?))
}

pub async fn events(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    db::get_by_slug(&state.pool, &slug).await?;
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(move |msg| match msg {
        Ok(changed) if changed == slug => {
            Some(Ok(Event::default().event("update").data("changed")))
        }
        _ => None,
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn appearance(State(state): State<AppState>) -> Result<Json<Appearance>, ApiError> {
    Ok(Json(settings::load(&state.pool).await?))
}

pub async fn activity(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Vec<LogEvent>>, ApiError> {
    let t = db::get_by_slug(&state.pool, &slug).await?;
    Ok(Json(events::list(&state.pool, t.id).await?))
}

pub async fn calendar(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let t = db::get_by_slug(&state.pool, &slug).await?;
    let body = crate::calendar::ics(&t, crate::tokens::now());
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/calendar; charset=utf-8",
        )],
        body,
    ))
}
