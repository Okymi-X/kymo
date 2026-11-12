//! Lab hosts, for admins: add one, check it, see what it runs, clean it up, and choose which
//! one an event uses. The token is accepted here and never returned.

use std::collections::HashSet;
use std::time::Instant;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{Require, needs};
use crate::error::ApiError;
use crate::lab_store::{self as store, LabFields, LabHostRow};
use crate::state::AppState;
use crate::{event_store, instance_store, labs, monitor_store};

#[derive(Deserialize)]
pub struct CreateBody {
    name: String,
    api_url: String,
    token: String,
    #[serde(default)]
    fingerprint: String,
    public_host: String,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    name: Option<String>,
    api_url: Option<String>,
    /// Left out or empty keeps the stored token.
    token: Option<String>,
    /// Empty removes the pin.
    fingerprint: Option<String>,
    public_host: Option<String>,
    enabled: Option<bool>,
}

async fn show(state: &AppState, row: LabHostRow) -> Result<Value, ApiError> {
    let (events, live) = store::counts(&state.pool, row.id).await?;
    serde_json::to_value(row.out(events, live)).map_err(|_| ApiError::Internal)
}

pub async fn list(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let mut out = Vec::new();
    for row in store::list(&state.pool).await? {
        out.push(show(&state, row).await?);
    }
    Ok(Json(json!({ "labs": out })))
}

/// Words for a connection failure that tell an operator what to change.
fn explain(error: &str) -> String {
    if error.contains("certificate") || error.contains("UnknownIssuer") {
        format!(
            "{error}. A lab host installed with the installer has a self-signed certificate: paste the fingerprint it printed (kymo-lab fingerprint)."
        )
    } else {
        error.to_owned()
    }
}

/// Asks the lab host for its health and what it will run, and remembers the answer.
async fn check(state: &AppState, row: &LabHostRow) -> Result<Value, ApiError> {
    let client = state.labs().client(row)?;
    let started = Instant::now();
    let result = client.health().await;
    let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    match result {
        Ok(health) => {
            let artifacts = client.artifacts().await.unwrap_or_default();
            store::record_check(&state.pool, row.id, state.now(), None).await?;
            store::record_vpn(&state.pool, row.id, health.vpn.as_ref(), health.vpn_only).await?;
            let mut notes = Vec::new();
            if health.mode == "manifest" && artifacts.is_empty() {
                notes.push("This lab host is in manifest mode and has no manifests, so it will refuse every start. Add manifest files on the lab host.".to_owned());
            }
            if row.api_url.starts_with("http://") {
                notes.push("This connection is not encrypted: the token and instance details cross the network in clear text.".to_owned());
            }
            Ok(
                json!({ "ok": true, "ms": ms, "health": health, "artifacts": artifacts, "notes": notes }),
            )
        }
        Err(e) => {
            let message = explain(&e.to_string());
            store::record_check(&state.pool, row.id, state.now(), Some(&message)).await?;
            Ok(json!({ "ok": false, "ms": ms, "error": message }))
        }
    }
}

pub async fn create(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let name = store::check_name(&body.name)?;
    let url = store::check_url(&body.api_url)?;
    let token = store::check_token(&body.token)?;
    let fingerprint = store::check_fingerprint(&body.fingerprint)?;
    if fingerprint.is_some() && !url.starts_with("https://") {
        return Err(ApiError::BadRequest(
            "a pinned fingerprint needs an https:// URL".into(),
        ));
    }
    let host = store::check_host(&body.public_host)?;
    let id = store::insert(
        &state.pool,
        &name,
        &url,
        &token,
        fingerprint.as_deref(),
        &host,
        state.now(),
    )
    .await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "lab_create",
        "lab_host",
        id,
        &format!("{name} {url}"),
    )
    .await?;
    let row = store::get(&state.pool, id).await?;
    // Saved even if it cannot be reached yet: the answer says why, and it can be fixed in place.
    let checked = check(&state, &row).await?;
    let row = store::get(&state.pool, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "lab": show(&state, row).await?, "check": checked })),
    ))
}

pub async fn update(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<Value>, ApiError> {
    let fields = LabFields {
        name: body.name,
        api_url: body.api_url,
        token: body.token,
        fingerprint: body.fingerprint,
        public_host: body.public_host,
        enabled: body.enabled,
    };
    store::update(&state.pool, id, &fields).await?;
    state.labs().forget(id);
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "lab_update",
        "lab_host",
        id,
        "",
    )
    .await?;
    Ok(Json(
        json!({ "lab": show(&state, store::get(&state.pool, id).await?).await? }),
    ))
}

pub async fn delete(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.write_lock.lock().await;
    let row = store::get(&state.pool, id).await?;
    store::delete(&state.pool, id).await?;
    state.labs().forget(id);
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "lab_delete",
        "lab_host",
        id,
        &row.name,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = store::get(&state.pool, id).await?;
    Ok(Json(check(&state, &row).await?))
}

/// What the lab host is running, with a flag for each instance the platform does not know.
pub async fn instances(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = store::get(&state.pool, id).await?;
    let client = state.labs().client(&row)?;
    let listed = client
        .instances()
        .await
        .map_err(|e| ApiError::Unavailable(explain(&e.to_string())))?;
    let wanted: HashSet<String> = instance_store::live(&state.pool)
        .await?
        .into_iter()
        .filter(|r| r.lab_host_id == Some(id))
        .map(|r| r.id.to_string())
        .collect();
    let list: Vec<Value> = listed
        .into_iter()
        .map(|i| {
            let known = wanted.contains(&i.instance_id);
            let mut v = serde_json::to_value(i).unwrap_or(Value::Null);
            v["known"] = json!(known);
            v
        })
        .collect();
    Ok(Json(json!({ "instances": list })))
}

/// A virtual machine's console on a lab host, as a PNG, for an organizer working out why a
/// machine does not answer (stuck at a prompt, no network). The lab host's API is not reachable
/// from a browser; the platform asks for it.
pub async fn screen(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Path((id, name)): Path<(i64, String)>,
) -> Result<axum::response::Response, ApiError> {
    use axum::http::{HeaderValue, header};
    use axum::response::IntoResponse;
    let row = store::get(&state.pool, id).await?;
    let client = state.labs().client(&row)?;
    let png = client
        .screen(&name)
        .await
        .map_err(|e| ApiError::Unavailable(explain(&e.to_string())))?;
    let mut response = png.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

/// Tells the lab host to drop everything the platform no longer lists.
pub async fn reconcile(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let row = store::get(&state.pool, id).await?;
    let client = state.labs().client(&row)?;
    let _guard = state.write_lock.lock().await;
    let keep: Vec<String> = instance_store::live(&state.pool)
        .await?
        .into_iter()
        .filter(|r| r.lab_host_id == Some(id))
        .map(|r| r.id.to_string())
        .collect();
    let removed = client
        // No grace: the write lock is held, so no start can add a run while this goes on, and the
        // list above already includes every start that is under way.
        .reconcile(keep, Some(0))
        .await
        .map_err(|e| ApiError::Unavailable(explain(&e.to_string())))?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "lab_reconcile",
        "lab_host",
        id,
        &format!("{} removed", removed.len()),
    )
    .await?;
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Deserialize)]
pub struct EventLabBody {
    /// The lab host to use, or null for this machine's own runtime.
    lab_host_id: Option<i64>,
    /// Set together with the lab host, because the two must agree: the VPN needs a gateway where
    /// the event lives, and a VPN-only lab host refuses published ports. Left out keeps the mode.
    #[serde(default)]
    network_mode: Option<String>,
}

/// Chooses where an event's instances run. Not while instances are running, because their
/// players were told one address and a stop would go to the wrong machine.
pub async fn set_event_lab(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<EventLabBody>,
) -> Result<Json<Value>, ApiError> {
    let event = event_store::by_id(&state.pool, id).await?;
    if let Some(lab) = body.lab_host_id {
        let row = labs::usable(&state, lab).await.map_err(|e| match e {
            ApiError::NotFound => ApiError::BadRequest("that lab host does not exist".into()),
            other => other,
        })?;
        let _ = row;
    }
    // The event's network mode must work on its new home: a gateway for the VPN, and published
    // ports only where the lab host allows them.
    let mode = body
        .network_mode
        .clone()
        .unwrap_or_else(|| event.network_mode.clone());
    if !event_store::NETWORK_MODES.contains(&mode.as_str()) {
        return Err(ApiError::BadRequest("mode is published or vpn".into()));
    }
    crate::vpn::check_mode(&state, body.lab_host_id, &mode).await?;
    if event.network_mode != mode {
        crate::vpn::check_overrides(&state, id, &mode).await?;
    }
    if event.lab_host_id != body.lab_host_id || event.network_mode != mode {
        let _guard = state.write_lock.lock().await;
        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM instance WHERE event_id = ? AND state IN ('starting', 'running')",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
        if live > 0 {
            return Err(ApiError::Conflict(
                "stop the running instances before moving the event to another lab host".into(),
            ));
        }
        event_store::set_lab_host(&state.pool, id, body.lab_host_id).await?;
        if event.network_mode != mode {
            event_store::set_network_mode(&state.pool, id, &mode).await?;
        }
        monitor_store::audit(
            &state.pool,
            state.now(),
            Some(&who.0),
            "event_lab",
            "event",
            id,
            &format!("{:?}", body.lab_host_id),
        )
        .await?;
    }
    Ok(Json(
        json!({ "event_id": id, "lab_host_id": body.lab_host_id, "network_mode": mode }),
    ))
}

#[derive(Deserialize)]
pub struct ChallengeLabBody {
    lab_host_id: Option<i64>,
}

/// Chooses where one challenge's instances run, overriding the event's host; none follows the
/// event again. The event's network mode must work there, and not while its instances run.
pub async fn set_challenge_lab(
    who: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ChallengeLabBody>,
) -> Result<Json<Value>, ApiError> {
    let challenge = crate::challenge_store::challenge_by_id(&state.pool, id).await?;
    let event = event_store::by_id(&state.pool, challenge.event_id).await?;
    if event.kind != "jeopardy" {
        return Err(ApiError::Conflict(
            "only Jeopardy events have challenge instances".into(),
        ));
    }
    if let Some(lab) = body.lab_host_id {
        labs::usable(&state, lab).await.map_err(|e| match e {
            ApiError::NotFound => ApiError::BadRequest("that lab host does not exist".into()),
            other => other,
        })?;
    }
    crate::vpn::check_mode(
        &state,
        body.lab_host_id.or(event.lab_host_id),
        &event.network_mode,
    )
    .await?;
    if challenge.lab_host_id != body.lab_host_id {
        let _guard = state.write_lock.lock().await;
        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM instance WHERE challenge_id = ? AND state IN ('starting', 'running')",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
        if live > 0 {
            return Err(ApiError::Conflict(
                "stop this challenge's running instances before moving it to another lab host"
                    .into(),
            ));
        }
        crate::challenge_store::set_lab_host(&state.pool, id, body.lab_host_id).await?;
        monitor_store::audit(
            &state.pool,
            state.now(),
            Some(&who.0),
            "challenge_lab",
            "challenge",
            id,
            &format!("{:?}", body.lab_host_id),
        )
        .await?;
    }
    Ok(Json(
        json!({ "challenge_id": id, "lab_host_id": body.lab_host_id }),
    ))
}
