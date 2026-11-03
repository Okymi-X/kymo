//! VPN endpoints: a player downloads their own profile; staff see the gateway, switch an
//! event between published ports and the VPN, and revoke a player.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{ClientIp, Player, Require, needs};
use crate::error::ApiError;
use crate::event_store;
use crate::monitor_store;
use crate::state::AppState;
use crate::vpn::{self, lab_subnet};
use crate::vpn_store as store;

#[derive(Deserialize, Default)]
pub struct HostQuery {
    /// The lab host whose gateway the profile is for; none is this server's own.
    lab_host: Option<i64>,
}

/// What a player needs to know before they download a profile.
pub async fn status(
    player: Player,
    State(state): State<AppState>,
    Query(q): Query<HostQuery>,
) -> Result<Json<Value>, ApiError> {
    if state.vpn_setup().is_none() {
        return Ok(Json(json!({ "enabled": false })));
    }
    let hid = q.lab_host.unwrap_or(0);
    let setup = match vpn::setup_for(&state, hid).await {
        Ok(s) => s,
        // The gateway of this host cannot be reached or does not exist: say so, do not hide it.
        Err(ApiError::Unavailable(reason)) => {
            return Ok(Json(
                json!({ "enabled": true, "available": false, "reason": reason }),
            ));
        }
        Err(e) => return Err(e),
    };
    let client = store::client(&state.pool, player.id, hid).await?;
    let tunnel_ip = client
        .as_ref()
        .and_then(|c| u32::try_from(c.idx).ok())
        .and_then(|i| setup.tunnel.client(i))
        .map(|ip| ip.to_string());
    Ok(Json(json!({
        "enabled": true,
        "available": true,
        "host": setup.public_host,
        "port": setup.public_port,
        "has_profile": client.is_some(),
        "tunnel_ip": tunnel_ip,
        "issued_at": client.as_ref().map(|c| c.issued_at),
        "expires_at": client.as_ref().map(|c| c.expires_at),
    })))
}

/// Issues a new certificate and returns the `.ovpn` file. The previous profile for the same
/// host stops working. A POST because it changes state; the response is never cached.
pub async fn profile(
    player: Player,
    ip: ClientIp,
    State(state): State<AppState>,
    Query(q): Query<HostQuery>,
) -> Result<Response, ApiError> {
    let hid = q.lab_host.unwrap_or(0);
    let profile = vpn::issue_profile(&state, player.id, hid).await?;
    // Every new profile revokes the old one: worth knowing who asked, and from where.
    let _ = monitor_store::audit_from(
        &state.pool,
        state.now(),
        Some(player.id),
        "player",
        Some(&ip.0.to_string()),
        "vpn_profile",
        "lab_host",
        hid,
        &format!("tunnel_ip={}", profile.tunnel_ip),
    )
    .await;
    let mut response = profile.text.into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-openvpn-profile"),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"kymo.ovpn\""),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    if let Ok(value) = HeaderValue::from_str(&profile.tunnel_ip) {
        headers.insert("x-kymo-tunnel-ip", value);
    }
    Ok(response)
}

pub async fn admin_status(
    _: Require<needs::Monitor>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let Some(base) = state.vpn_setup() else {
        return Ok(Json(json!({ "enabled": false })));
    };
    let gateway = if base.local {
        match base.gateway.status().await {
            Ok(s) => json!({ "running": s.running, "networks": s.networks, "error": null }),
            Err(e) => json!({ "running": false, "networks": [], "error": e.to_string() }),
        }
    } else {
        json!({ "running": false, "networks": [], "error": null, "none": true })
    };
    let labs = store::labs(&state.pool).await?;
    let mut lab_list = Vec::new();
    for lab in &labs {
        // Name the owner: a handle or a team name reads better than "user 4".
        let name: Option<String> = if lab.scorer_kind == "team" {
            sqlx::query_scalar("SELECT name FROM player_team WHERE id = ?").bind(lab.scorer_id)
        } else {
            sqlx::query_scalar("SELECT username FROM user WHERE id = ?").bind(lab.scorer_id)
        }
        .fetch_optional(&state.pool)
        .await?;
        lab_list.push(json!({
            "id": lab.id,
            "lab_host_id": lab.lab_host_id,
            "event_id": lab.event_id,
            "participant": { "kind": lab.scorer_kind, "id": lab.scorer_id, "name": name },
            "network": lab.network,
            "subnet": lab_subnet(base, lab)?.to_string(),
            "live_instances": store::live_instances_in_lab(&state.pool, lab).await?,
            "last_used_at": lab.last_used_at,
        }));
    }
    // Each lab host's own gateway.
    let mut hosts = Vec::new();
    for row in crate::lab_store::list(&state.pool).await? {
        let clients = store::clients(&state.pool, row.id).await?.len();
        let host_labs = labs.iter().filter(|l| l.lab_host_id == row.id).count();
        let (info, gateway) = match vpn::setup_for(&state, row.id).await {
            Ok(setup) => (
                json!({ "host": setup.public_host, "port": setup.public_port }),
                match setup.gateway.status().await {
                    Ok(s) => json!({ "running": s.running, "networks": s.networks, "error": null }),
                    Err(e) => json!({ "running": false, "networks": [], "error": e.to_string() }),
                },
            ),
            Err(e) => (
                json!(null),
                json!({ "running": false, "networks": [], "error": api_reason(&e) }),
            ),
        };
        hosts.push(json!({
            "id": row.id,
            "name": row.name,
            "enabled": row.enabled,
            "vpn": info,
            "gateway": gateway,
            "labs": host_labs,
            "clients": clients,
        }));
    }
    let clients = store::clients(&state.pool, 0).await?;
    Ok(Json(json!({
        "enabled": true,
        "local": base.local,
        "host": base.public_host,
        "port": base.public_port,
        "gateway": gateway,
        "tunnel": { "net": base.tunnel.net().to_string(), "capacity": base.tunnel.capacity(), "used": clients.len() },
        "lab_pool": { "net": base.labs.pool().to_string(), "capacity": base.labs.capacity(), "used": labs.len() },
        "labs": lab_list,
        "clients": clients.len(),
        "hosts": hosts,
    })))
}

/// The words of an error, for showing next to the lab host that caused it.
fn api_reason(e: &ApiError) -> String {
    match e {
        ApiError::Unavailable(m) | ApiError::Conflict(m) | ApiError::BadRequest(m) => m.clone(),
        other => format!("{other:?}"),
    }
}

pub async fn admin_sync(
    who: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let report = vpn::sync(&state).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "vpn_sync",
        "vpn",
        "-",
        "",
    )
    .await?;
    Ok(Json(json!({
        "labs": report.labs,
        "clients": report.clients,
        "allows": report.allows,
        "disconnected": report.removed_clients,
    })))
}

pub async fn admin_revoke(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if !vpn::revoke_client(&state, user_id).await? {
        return Err(ApiError::NotFound);
    }
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "vpn_revoke",
        "user",
        user_id,
        "",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct NetworkBody {
    mode: String,
}

/// Switches an event between published ports and the VPN. Not while instances are running,
/// because their players would be told one address and be sent to another.
pub async fn set_network(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<NetworkBody>,
) -> Result<Json<Value>, ApiError> {
    if !event_store::NETWORK_MODES.contains(&body.mode.as_str()) {
        return Err(ApiError::BadRequest("mode is published or vpn".into()));
    }
    let event = event_store::by_id(&state.pool, id).await?;
    if event.kind != "jeopardy" {
        return Err(ApiError::Conflict(
            "only Jeopardy events have challenge instances".into(),
        ));
    }
    vpn::check_mode(&state, event.lab_host_id, &body.mode).await?;
    if event.network_mode != body.mode {
        let _guard = state.write_lock.lock().await;
        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM instance WHERE event_id = ? AND state IN ('starting', 'running')",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
        if live > 0 {
            return Err(ApiError::Conflict(
                "stop the running instances before changing the network mode".into(),
            ));
        }
        event_store::set_network_mode(&state.pool, id, &body.mode).await?;
        monitor_store::audit(
            &state.pool,
            state.now(),
            Some(&who.0),
            "event_network",
            "event",
            id,
            &body.mode,
        )
        .await?;
    }
    Ok(Json(json!({ "event_id": id, "network_mode": body.mode })))
}
