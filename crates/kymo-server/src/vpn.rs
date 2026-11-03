//! Per player OpenVPN access to private lab networks. This module keeps three things in step
//! with the database: the files the gateway reads (certificates, per player address files,
//! the firewall script), the lab networks, and the gateway's attachments to them. `sync` is
//! idempotent, so it runs after every change and again on a timer, and heals any drift.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kymo_instances::InstanceRuntime;
use kymo_vpn::{
    Allow, Ca, CaMaterial, ClientProfile, Gateway, GatewayConfig, Ipv4Net, Issued, LabPool,
    Revoked, TunnelPool, client_config, client_profile, firewall_script, generate_tls_crypt_key,
    server_conf, valid_common_name,
};

use crate::error::ApiError;
use crate::jeopardy_store::Scorer;
use crate::state::AppState;
use crate::vpn_store::{
    self as store, ClientRow, LabRow, SETTING_CA, SETTING_CRL_COUNT, SETTING_CRL_NUMBER,
    SETTING_SERVER, SETTING_TLS_CRYPT,
};

/// Seconds a lab with no running instance is kept before it is removed.
pub const IDLE_LAB_SECS: i64 = 600;
const DAY: i64 = 86_400;

/// Everything the VPN needs that is not in the database.
#[derive(Clone)]
pub struct VpnSetup {
    pub gateway: Arc<dyn Gateway>,
    pub runtime: Arc<dyn InstanceRuntime>,
    /// Host directory the gateway mounts at /vpn.
    pub dir: PathBuf,
    pub gateway_config: GatewayConfig,
    /// Where players connect, written into their profiles.
    pub public_host: String,
    pub public_port: u16,
    pub tunnel: TunnelPool,
    pub labs: LabPool,
    /// Lifetime of a player's certificate.
    pub client_days: i64,
    /// Serializes `sync` so two changes at once cannot interleave their file writes.
    pub lock: Arc<tokio::sync::Mutex<()>>,
}

fn setup(state: &AppState) -> Result<&VpnSetup, ApiError> {
    state
        .vpn_setup()
        .ok_or_else(|| ApiError::Unavailable("the VPN is not enabled on this server".into()))
}

fn internal(what: &str, e: impl std::fmt::Display) -> ApiError {
    tracing::error!("vpn: {what}: {e}");
    ApiError::Internal
}

fn gateway_down(e: impl std::fmt::Display) -> ApiError {
    tracing::warn!("vpn gateway: {e}");
    ApiError::Unavailable("the VPN gateway is not available, try again shortly".into())
}

pub fn common_name(user_id: i64) -> String {
    format!("u{user_id}")
}

// ---------- files ----------

/// Writes a file so the gateway never sees half of it: a temporary file, then a rename.
fn write_atomic(path: &Path, text: &str, secret: bool) -> Result<(), ApiError> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| internal("create directory", e))?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if secret { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = secret;
    let mut file = options.open(&tmp).map_err(|e| internal("open", e))?;
    file.write_all(text.as_bytes())
        .map_err(|e| internal("write", e))?;
    file.sync_all().map_err(|e| internal("sync", e))?;
    std::fs::rename(&tmp, path).map_err(|e| internal("rename", e))
}

// ---------- the authority ----------

struct Material {
    ca: Ca,
    ta: String,
}

async fn authority(state: &AppState, setup: &VpnSetup) -> Result<Material, ApiError> {
    let _ = setup;
    let now = state.now();
    let ca = match store::setting(&state.pool, SETTING_CA).await? {
        Some(json) => {
            let material: CaMaterial =
                serde_json::from_str(&json).map_err(|e| internal("read the CA", e))?;
            Ca::load(material).map_err(|e| internal("load the CA", e))?
        }
        None => {
            let material = Ca::generate(now, 3650).map_err(|e| internal("make the CA", e))?;
            let json = serde_json::to_string(&material).map_err(|e| internal("store the CA", e))?;
            store::set_setting(&state.pool, SETTING_CA, &json).await?;
            Ca::load(material).map_err(|e| internal("load the CA", e))?
        }
    };
    let ta = match store::setting(&state.pool, SETTING_TLS_CRYPT).await? {
        Some(key) => key,
        None => {
            let key = generate_tls_crypt_key();
            store::set_setting(&state.pool, SETTING_TLS_CRYPT, &key).await?;
            key
        }
    };
    Ok(Material { ca, ta })
}

/// The gateway's own certificate, reissued when it is within a month of expiring.
async fn server_certificate(state: &AppState, ca: &Ca) -> Result<Issued, ApiError> {
    let now = state.now();
    let stored = store::setting(&state.pool, SETTING_SERVER).await?;
    if let Some((issued, expires)) =
        stored.and_then(|json| serde_json::from_str::<(Issued, i64)>(&json).ok())
        && expires - now > 30 * DAY
    {
        return Ok(issued);
    }
    let issued = ca
        .issue_server("kymo-gateway", now, 365)
        .map_err(|e| internal("issue the gateway certificate", e))?;
    let json = serde_json::to_string(&(&issued, now + 365 * DAY))
        .map_err(|e| internal("store the gateway certificate", e))?;
    store::set_setting(&state.pool, SETTING_SERVER, &json).await?;
    Ok(issued)
}

async fn write_crl(state: &AppState, setup: &VpnSetup, ca: &Ca) -> Result<(), ApiError> {
    let revoked = store::revoked(&state.pool).await?;
    let path = setup.dir.join("pki/crl.pem");
    let stored: usize = store::setting(&state.pool, SETTING_CRL_COUNT)
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    if path.exists() && stored == revoked.len() {
        return Ok(());
    }
    let number: u64 = store::setting(&state.pool, SETTING_CRL_NUMBER)
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        + 1;
    let list: Vec<Revoked> = revoked
        .into_iter()
        .map(|(serial, at)| Revoked { serial, at })
        .collect();
    let pem = ca
        .crl(&list, number, state.now(), 365)
        .map_err(|e| internal("make the revocation list", e))?;
    write_atomic(&path, &pem, false)?;
    store::set_setting(&state.pool, SETTING_CRL_NUMBER, &number.to_string()).await?;
    store::set_setting(&state.pool, SETTING_CRL_COUNT, &list.len().to_string()).await
}

// ---------- labs ----------

/// The lab of a participant, made (network, gateway attachment) when it does not exist yet.
pub async fn lab_for(state: &AppState, event_id: i64, scorer: Scorer) -> Result<LabRow, ApiError> {
    let setup = setup(state)?;
    let now = state.now();
    let lab = match store::lab_of(&state.pool, event_id, scorer).await? {
        Some(lab) => lab,
        None => {
            let used = store::used_lab_indices(&state.pool).await?;
            let idx = setup.labs.next_free(&used).map_err(|_| {
                ApiError::Unavailable("every lab network is in use, try again later".into())
            })?;
            store::insert_lab(
                &state.pool,
                idx,
                event_id,
                scorer,
                &format!("kymo-lab-{idx}"),
                now,
            )
            .await?
        }
    };
    let subnet = lab_subnet(setup, &lab)?;
    setup
        .runtime
        .ensure_lab(&lab.network, &subnet.to_string())
        .await
        .map_err(|e| {
            tracing::warn!("making lab {}: {e}", lab.network);
            ApiError::Unavailable("could not prepare your private network, try again later".into())
        })?;
    setup
        .gateway
        .attach(&lab.network)
        .await
        .map_err(gateway_down)?;
    store::touch_lab(&state.pool, lab.id, now).await?;
    Ok(lab)
}

pub fn lab_subnet(setup: &VpnSetup, lab: &LabRow) -> Result<Ipv4Net, ApiError> {
    u32::try_from(lab.idx)
        .ok()
        .and_then(|i| setup.labs.lab(i))
        .ok_or(ApiError::Internal)
}

/// The next address for a new instance in a lab. Addresses are handed out in turn, not lowest
/// first, so a new instance never reuses the address of the one that just ended: a stale
/// connection or a bookmark then cannot land on a different challenge by accident.
pub async fn free_lab_ip(state: &AppState, lab: &LabRow) -> Result<String, ApiError> {
    let setup = setup(state)?;
    let in_use = store::lab_ips_in_use(&state.pool, lab.id).await?;
    let subnet = lab_subnet(setup, lab)?;
    let slots: Vec<String> = setup
        .labs
        .instance_slots(&subnet)
        .into_iter()
        .map(|ip| ip.to_string())
        .collect();
    let last = store::last_lab_ip(&state.pool, lab.id).await?;
    let start = last
        .and_then(|ip| slots.iter().position(|s| *s == ip))
        .map_or(0, |i| i + 1);
    (0..slots.len())
        .map(|k| &slots[(start + k) % slots.len()])
        .find(|ip| !in_use.contains(*ip))
        .cloned()
        .ok_or_else(|| {
            ApiError::Conflict("your private network is full, stop another instance first".into())
        })
}

/// Whether the service at a lab address accepts connections, asked from inside the gateway.
pub async fn lab_reachable(state: &AppState, ip: &str, port: u16) -> bool {
    let (Some(setup), Ok(ip)) = (state.vpn_setup(), ip.parse::<std::net::Ipv4Addr>()) else {
        return false;
    };
    setup.gateway.reachable(ip, port).await.unwrap_or(false)
}

// ---------- profiles ----------

pub struct Profile {
    pub text: String,
    pub tunnel_ip: String,
    pub expires_at: i64,
}

/// Issues a fresh certificate for a player. Any older profile of theirs stops working.
pub async fn issue_profile(state: &AppState, user_id: i64) -> Result<Profile, ApiError> {
    let setup = setup(state)?;
    if store::is_banned(&state.pool, user_id).await? {
        return Err(ApiError::Banned);
    }
    let now = state.now();
    let material = authority(state, setup).await?;
    let cn = common_name(user_id);
    let issued = material
        .ca
        .issue_client(&cn, now, setup.client_days)
        .map_err(|e| internal("issue a client certificate", e))?;
    let expires_at = now + setup.client_days * DAY;
    let idx = match store::client(&state.pool, user_id).await? {
        Some(existing) => u32::try_from(existing.idx).map_err(|_| ApiError::Internal)?,
        None => {
            let used = store::used_client_indices(&state.pool).await?;
            setup
                .tunnel
                .next_free(&used)
                .map_err(|_| ApiError::Unavailable("no tunnel addresses are left".into()))?
        }
    };
    store::upsert_client(&state.pool, user_id, idx, &issued.serial, now, expires_at).await?;
    let tunnel_ip = setup.tunnel.client(idx).ok_or(ApiError::Internal)?;
    let text = client_profile(&ClientProfile {
        remote_host: &setup.public_host,
        remote_port: setup.public_port,
        ca_pem: material.ca.cert_pem(),
        cert_pem: &issued.cert_pem,
        key_pem: &issued.key_pem,
        tls_crypt_key: &material.ta,
    })
    .map_err(|e| internal("render the profile", e))?;
    // The old certificate and the new address file take effect at once.
    sync_quiet(state).await;
    Ok(Profile {
        text,
        tunnel_ip: tunnel_ip.to_string(),
        expires_at,
    })
}

/// Takes a player off the VPN: the certificate is revoked and their session ends.
pub async fn revoke_client(state: &AppState, user_id: i64) -> Result<bool, ApiError> {
    setup(state)?;
    let removed = store::remove_client(&state.pool, user_id, state.now()).await?;
    sync_quiet(state).await;
    Ok(removed)
}

// ---------- sync ----------

#[derive(Debug, Default)]
pub struct SyncReport {
    pub labs: usize,
    pub clients: usize,
    pub allows: usize,
    pub removed_clients: usize,
}

/// Brings the gateway in line with the database. Safe to call at any time.
pub async fn sync(state: &AppState) -> Result<SyncReport, ApiError> {
    let setup = setup(state)?;
    let _guard = setup.lock.lock().await;
    let material = authority(state, setup).await?;
    let server = server_certificate(state, &material.ca).await?;

    // Static files the gateway reads.
    let dir = &setup.dir;
    write_atomic(&dir.join("pki/ca.crt"), material.ca.cert_pem(), false)?;
    write_atomic(&dir.join("pki/server.crt"), &server.cert_pem, false)?;
    write_atomic(&dir.join("pki/server.key"), &server.key_pem, true)?;
    write_atomic(&dir.join("pki/ta.key"), &material.ta, true)?;
    write_crl(state, setup, &material.ca).await?;
    write_atomic(
        &dir.join("server.conf"),
        &server_conf(&setup.tunnel, &setup.labs.pool()),
        false,
    )?;

    // Which players exist and may connect.
    let mut desired: HashMap<i64, ClientRow> = HashMap::new();
    for client in store::clients(&state.pool).await? {
        if !store::is_banned(&state.pool, client.user_id).await? {
            desired.insert(client.user_id, client);
        }
    }
    let ccd = dir.join("ccd");
    std::fs::create_dir_all(&ccd).map_err(|e| internal("create ccd", e))?;
    let mut wanted_files = BTreeSet::new();
    for (user_id, client) in &desired {
        let idx = u32::try_from(client.idx).map_err(|_| ApiError::Internal)?;
        let ip = setup.tunnel.client(idx).ok_or(ApiError::Internal)?;
        let name = common_name(*user_id);
        write_atomic(&ccd.join(&name), &client_config(ip, &setup.tunnel), false)?;
        wanted_files.insert(name);
    }
    // A player with no file may not connect; anyone connected under such a name is dropped.
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(&ccd)
        .map_err(|e| internal("read ccd", e))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if valid_common_name(&name) && !wanted_files.contains(&name) {
            let _ = std::fs::remove_file(entry.path());
            removed.push(name);
        } else if name.ends_with(".tmp") {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    // Labs: the network exists, the gateway is on it, and the right players may reach it.
    let labs = store::labs(&state.pool).await?;
    let mut allows = Vec::new();
    for lab in &labs {
        let subnet = lab_subnet(setup, lab)?;
        for user_id in store::lab_members(&state.pool, &lab.scorer_kind, lab.scorer_id).await? {
            if let Some(client) = desired.get(&user_id) {
                let idx = u32::try_from(client.idx).map_err(|_| ApiError::Internal)?;
                if let Some(ip) = setup.tunnel.client(idx) {
                    allows.push(Allow {
                        client: ip,
                        lab: subnet,
                    });
                }
            }
        }
    }
    write_atomic(
        &dir.join("rules.sh"),
        &firewall_script(&allows, &setup.tunnel.net(), &setup.labs.pool()),
        false,
    )?;

    setup
        .gateway
        .ensure_running(&setup.gateway_config)
        .await
        .map_err(gateway_down)?;
    for lab in &labs {
        let subnet = lab_subnet(setup, lab)?;
        setup
            .runtime
            .ensure_lab(&lab.network, &subnet.to_string())
            .await
            .map_err(gateway_down)?;
        setup
            .gateway
            .attach(&lab.network)
            .await
            .map_err(gateway_down)?;
    }
    setup.gateway.apply_rules().await.map_err(gateway_down)?;
    for name in &removed {
        let _ = setup.gateway.kick(name).await;
    }
    Ok(SyncReport {
        labs: labs.len(),
        clients: desired.len(),
        allows: allows.len(),
        removed_clients: removed.len(),
    })
}

/// `sync` for callers that must not fail because the VPN is down or switched off.
pub async fn sync_quiet(state: &AppState) {
    if state.vpn_setup().is_none() {
        return;
    }
    if let Err(e) = sync(state).await {
        tracing::warn!("vpn sync failed: {e:?}");
    }
}

/// Removes labs nobody uses: the event is over (or gone), or no instance has run for a while.
pub async fn collect_idle_labs(state: &AppState) -> Result<usize, ApiError> {
    let Some(setup) = state.vpn_setup() else {
        return Ok(0);
    };
    let now = state.now();
    let mut removed = 0;
    for lab in store::labs(&state.pool).await? {
        if store::live_instances_in_lab(&state.pool, &lab).await? > 0 {
            store::touch_lab(&state.pool, lab.id, now).await?;
            continue;
        }
        let over = match crate::event_store::by_id(&state.pool, lab.event_id).await {
            Ok(event) => matches!(
                event.effective_state(now)?,
                kymo_core::EventState::Ended | kymo_core::EventState::Archived
            ),
            Err(_) => true,
        };
        if !over && now - lab.last_used_at < IDLE_LAB_SECS {
            continue;
        }
        if setup.gateway.detach(&lab.network).await.is_err() {
            continue;
        }
        if setup.runtime.remove_lab(&lab.network).await.is_err() {
            // Something is still in it; try again on the next pass.
            let _ = setup.gateway.attach(&lab.network).await;
            continue;
        }
        store::delete_lab(&state.pool, lab.id).await?;
        removed += 1;
    }
    if removed > 0 {
        sync_quiet(state).await;
    }
    Ok(removed)
}
