//! Operations endpoints: health, backups, cleanup, and event bundles.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{Require, needs};
use crate::error::ApiError;
use crate::event_bundle::{self, Bundle, ImportOptions};
use crate::event_store::{self, EventOut};
use crate::monitor_store;
use crate::ops;
use crate::state::AppState;

/// Largest accepted bundle: up to 500 challenges with attachments.
pub const BUNDLE_BODY_LIMIT: usize = 256 * 1024 * 1024;

/// Liveness for load balancers: no details, no authentication.
pub async fn live(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    if ops::database_ok(&state.pool).await {
        (StatusCode::OK, Json(json!({ "status": "ok" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "down" })),
        )
    }
}

pub async fn health(
    _: Require<needs::Monitor>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(ops::report(&state).await?))
}

pub async fn cleanup(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<Json<ops::CleanupReport>, ApiError> {
    let report = ops::cleanup(&state).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "ops_cleanup",
        "ops",
        "-",
        "",
    )
    .await?;
    Ok(Json(report))
}

pub async fn list_backups(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let files = match state.backup_config() {
        Some(config) => ops::list_backups(config).await?,
        None => Vec::new(),
    };
    Ok(Json(json!({
        "configured": state.backup_config().is_some(),
        "every_secs": state.backup_config().map(|c| c.every_secs),
        "keep": state.backup_config().map(|c| c.keep),
        "backups": files,
    })))
}

pub async fn create_backup(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ops::BackupFile>), ApiError> {
    let file = ops::write_backup(&state).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "backup_create",
        "backup",
        &file.name,
        "",
    )
    .await?;
    Ok((StatusCode::CREATED, Json(file)))
}

/// The whole database as one file. It contains password hashes and session hashes, so it is
/// never cached, only the organizer role can fetch it, and the download is audited.
pub async fn download_backup(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let bytes = ops::snapshot_bytes(&state.pool).await?;
    let name = format!("kymo-backup-{}.db", state.now());
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "backup_download",
        "backup",
        &name,
        "",
    )
    .await?;
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
        .map_err(|_| ApiError::Internal)?;
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.sqlite3"),
    );
    headers.insert(CONTENT_DISPOSITION, disposition);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

pub async fn export_event(
    who: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let event = event_store::by_id(&state.pool, id).await?;
    let bundle = event_bundle::export(&state.pool, &event, state.now()).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "event_export",
        "event",
        id,
        "",
    )
    .await?;
    let disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}.kymo-event.json\"",
        event.slug
    ))
    .map_err(|_| ApiError::Internal)?;
    let mut response = Json(bundle).into_response();
    let headers = response.headers_mut();
    headers.insert(CONTENT_DISPOSITION, disposition);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

#[derive(Deserialize)]
pub struct ImportQuery {
    #[serde(default)]
    keep_times: bool,
    #[serde(default)]
    keep_state: bool,
}

pub async fn import_event(
    who: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<EventOut>), ApiError> {
    let bundle: Bundle = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("this is not a valid event bundle: {e}")))?;
    let options = ImportOptions {
        keep_times: query.keep_times,
        keep_state: query.keep_state,
        vpn_available: state.vpn_setup().is_some(),
    };
    let row = event_bundle::import(&state.pool, &bundle, options, state.now()).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "event_import",
        "event",
        row.id,
        &row.slug,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(EventOut::new(row, state.now())?)))
}
