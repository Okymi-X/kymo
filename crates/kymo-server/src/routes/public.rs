use std::convert::Infallible;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::db::{self, Tournament};
use crate::error::ApiError;
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
    Json(body): Json<LoginBody>,
) -> Result<Json<LoginOut>, ApiError> {
    if state.login_blocked() {
        return Err(ApiError::TooManyRequests);
    }
    let ok = state.verify_password(body.password).await;
    state.record_login(ok);
    if !ok {
        return Err(ApiError::Unauthorized);
    }
    let token = tokens::issue(&state.pool, Kind::Session, "").await?;
    Ok(Json(LoginOut { token }))
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Tournament>>, ApiError> {
    let mut all = db::list(&state.pool).await?;
    for t in &mut all {
        t.bracket = None;
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
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(move |msg| match msg {
        Ok(changed) if changed == slug => {
            Some(Ok(Event::default().event("update").data("changed")))
        }
        _ => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub async fn appearance(State(state): State<AppState>) -> Result<Json<Appearance>, ApiError> {
    Ok(Json(settings::load(&state.pool).await?))
}
