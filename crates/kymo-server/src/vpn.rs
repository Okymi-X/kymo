//! Per player OpenVPN access to private lab networks. This module keeps three things in step
//! with the database: the files the gateway reads (certificates, per player address files,
//! the firewall script), the lab networks, and the gateway's attachments to them. `sync` is
//! idempotent, so it runs after every change and again on a timer, and heals any drift.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kymo_instances::InstanceRuntime;
use kymo_lab::client::RemoteGateway;
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
    /// Whether this server runs a gateway of its own. False when every instance lives on a lab
    /// host, each of which has its own.
    pub local: bool,
}

fn setup(state: &AppState) -> Result<&VpnSetup, ApiError> {
    state
        .vpn_setup()
        .ok_or_else(|| ApiError::Unavailable("the VPN is not enabled on this server".into()))
}

/// The VPN of one host: this server's own gateway (`hid` 0) or the one on lab host `hid`. A lab
/// host's gateway is driven over its API; the authority stays here, one per host, so a
/// certificate for one lab host opens nothing on another.
pub async fn setup_for(state: &AppState, hid: i64) -> Result<VpnSetup, ApiError> {
    let base = setup(state)?;
    if hid == 0 {
        if !base.local {
            return Err(ApiError::Unavailable(
                "this server has no gateway of its own: the event needs a lab host".into(),
            ));
        }
        return Ok(base.clone());
    }
    let row = crate::lab_store::get(&state.pool, hid).await?;
    let client = state.labs().client(&row)?;
    let health = client.health().await.map_err(|e| {
        ApiError::Unavailable(format!("the lab host {} did not answer: {e}", row.name))
    })?;
    let Some(info) = health.vpn else {
        return Err(ApiError::Unavailable(format!(
            "the lab host {} has no VPN gateway (set KYMO_LAB_VPN_HOST on it)",
            row.name
        )));
    };
    let pool: Ipv4Net = info.pool.parse().map_err(|_| {
        ApiError::Unavailable(format!("the lab host {} reports a bad VPN range", row.name))
    })?;
    let ours = base.labs.pool();
    if !(pool.contains(ours.network()) && ours.prefix() >= pool.prefix()) {
        return Err(ApiError::Unavailable(format!(
            "the lab host {} gives lab networks {pool}, which does not cover this server's {ours} (KYMO_VPN_LABS)",
            row.name
        )));
    }
    let dir = base.dir.join("hosts").join(hid.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| internal("create the lab host directory", e))?;
    Ok(VpnSetup {
        gateway: Arc::new(RemoteGateway::new(Arc::clone(&client))),
        runtime: client,
        dir: dir.clone(),
        gateway_config: GatewayConfig {
            name: "kymo-lab-vpn-gateway".into(),
            image: String::new(),
            dir,
            publish_ip: String::new(),
            publish_port: info.port,
            bind_options: String::new(),
        },
        public_host: info.host,
        public_port: info.port,
        local: true,
        ..base.clone()
    })
}

/// Whether an event may use `mode` on `lab_host_id` (none is this server): the VPN needs a
/// gateway there, and a lab host that is VPN only never publishes a port. Says why not, in words
/// an organizer can act on.
pub async fn check_mode(
    state: &AppState,
    lab_host_id: Option<i64>,
    mode: &str,
) -> Result<(), ApiError> {
    let reason = |e: ApiError| match e {
        ApiError::Unavailable(m) | ApiError::Conflict(m) | ApiError::BadRequest(m) => {
            ApiError::Conflict(m)
        }
        other => other,
    };
    if mode == "vpn" {
        if state.vpn_setup().is_none() {
            return Err(ApiError::Conflict(
                "the VPN is not enabled on this server".into(),
            ));
        }
        return setup_for(state, lab_host_id.unwrap_or(0))
            .await
            .map(|_| ())
            .map_err(reason);
    }
    if let Some(id) = lab_host_id {
        let row = crate::lab_store::get(&state.pool, id).await?;
        // A lab host that cannot be asked right now is not a reason to refuse: starts will fail
        // plainly if it still is not there.
        if let Ok(health) = state.labs().client(&row)?.health().await
            && health.vpn_only
        {
            return Err(ApiError::Conflict(format!(
                "the lab host {} only runs instances through its VPN: switch the event to the VPN",
                row.name
            )));
        }
    }
    Ok(())
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

async fn authority(state: &AppState, hid: i64) -> Result<Material, ApiError> {
    let now = state.now();
    let (ca_key, ta_key) = (
        store::host_key(SETTING_CA, hid),
        store::host_key(SETTING_TLS_CRYPT, hid),
    );
    let ca = match store::setting(&state.pool, &ca_key).await? {
        Some(json) => {
            let material: CaMaterial =
                serde_json::from_str(&json).map_err(|e| internal("read the CA", e))?;
            Ca::load(material).map_err(|e| internal("load the CA", e))?
        }
        None => {
            let material = Ca::generate(now, 3650).map_err(|e| internal("make the CA", e))?;
            let json = serde_json::to_string(&material).map_err(|e| internal("store the CA", e))?;
            store::set_setting(&state.pool, &ca_key, &json).await?;
            Ca::load(material).map_err(|e| internal("load the CA", e))?
        }
    };
    let ta = match store::setting(&state.pool, &ta_key).await? {
        Some(key) => key,
        None => {
            let key = generate_tls_crypt_key();
            store::set_setting(&state.pool, &ta_key, &key).await?;
            key
        }
    };
    Ok(Material { ca, ta })
}

/// The gateway's own certificate, reissued when it is within a month of expiring.
async fn server_certificate(state: &AppState, ca: &Ca, hid: i64) -> Result<Issued, ApiError> {
    let now = state.now();
    let key = store::host_key(SETTING_SERVER, hid);
    let stored = store::setting(&state.pool, &key).await?;
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
    store::set_setting(&state.pool, &key, &json).await?;
    Ok(issued)
}

async fn write_crl(state: &AppState, setup: &VpnSetup, ca: &Ca, hid: i64) -> Result<(), ApiError> {
    let revoked = store::revoked(&state.pool, hid).await?;
    let (count_key, number_key) = (
        store::host_key(SETTING_CRL_COUNT, hid),
        store::host_key(SETTING_CRL_NUMBER, hid),
    );
    let path = setup.dir.join("pki/crl.pem");
    let stored: usize = store::setting(&state.pool, &count_key)
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    if path.exists() && stored == revoked.len() {
        return Ok(());
    }
    let number: u64 = store::setting(&state.pool, &number_key)
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
    store::set_setting(&state.pool, &number_key, &number.to_string()).await?;
    store::set_setting(&state.pool, &count_key, &list.len().to_string()).await
}

// ---------- labs ----------

/// The lab of a participant, made (network, gateway attachment) when it does not exist yet.
pub async fn lab_for(
    state: &AppState,
    event_id: i64,
    scorer: Scorer,
    hid: i64,
) -> Result<LabRow, ApiError> {
    let setup = setup_for(state, hid).await?;
    let setup = &setup;
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
                hid,
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
    // Joining the gateway to the lab is `sync_host`'s job, which the start flow runs next and
    // which also starts the gateway if it is not running yet (a new lab host's is not). A
    // failure here is therefore not final.
    if let Err(e) = setup.gateway.attach(&lab.network).await {
        tracing::debug!("gateway not on {} yet: {e}", lab.network);
    }
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
pub async fn lab_reachable(state: &AppState, hid: i64, ip: &str, port: u16) -> bool {
    let (Ok(setup), Ok(ip)) = (
        setup_for(state, hid).await,
        ip.parse::<std::net::Ipv4Addr>(),
    ) else {
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

/// Issues a fresh certificate for a player on one host. Any older profile of theirs for that
/// host stops working; their profiles for other hosts are unaffected.
pub async fn issue_profile(state: &AppState, user_id: i64, hid: i64) -> Result<Profile, ApiError> {
    let setup = setup_for(state, hid).await?;
    if store::is_banned(&state.pool, user_id).await? {
        return Err(ApiError::Banned);
    }
    let now = state.now();
    let material = authority(state, hid).await?;
    let cn = common_name(user_id);
    let issued = material
        .ca
        .issue_client(&cn, now, setup.client_days)
        .map_err(|e| internal("issue a client certificate", e))?;
    let expires_at = now + setup.client_days * DAY;
    let idx = match store::client(&state.pool, user_id, hid).await? {
        Some(existing) => u32::try_from(existing.idx).map_err(|_| ApiError::Internal)?,
        None => {
            let used = store::used_client_indices(&state.pool, hid).await?;
            setup
                .tunnel
                .next_free(&used)
                .map_err(|_| ApiError::Unavailable("no tunnel addresses are left".into()))?
        }
    };
    store::upsert_client(
        &state.pool,
        user_id,
        hid,
        idx,
        &issued.serial,
        now,
        expires_at,
    )
    .await?;
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
    if let Err(e) = sync_host(state, hid).await {
        tracing::warn!("vpn sync of host {hid} failed: {e:?}");
    }
    Ok(Profile {
        text,
        tunnel_ip: tunnel_ip.to_string(),
        expires_at,
    })
}

/// Takes a player off the VPN everywhere: their certificates are revoked and sessions end.
pub async fn revoke_client(state: &AppState, user_id: i64) -> Result<bool, ApiError> {
    setup(state)?;
    let hosts = store::remove_client(&state.pool, user_id, state.now()).await?;
    for hid in &hosts {
        if let Err(e) = sync_host(state, *hid).await {
            tracing::warn!("vpn sync of host {hid} failed: {e:?}");
        }
    }
    Ok(!hosts.is_empty())
}

// ---------- sync ----------

#[derive(Debug, Default)]
pub struct SyncReport {
    pub labs: usize,
    pub clients: usize,
    pub allows: usize,
    pub removed_clients: usize,
}

/// Brings every gateway in line with the database: this server's own, and each lab host's that
/// has players or labs. A host that does not answer does not hold up the others; the first
/// error is returned once all have been tried.
pub async fn sync(state: &AppState) -> Result<SyncReport, ApiError> {
    let base = setup(state)?;
    let mut hosts = store::hosts_in_use(&state.pool).await?;
    if base.local && !hosts.contains(&0) {
        hosts.insert(0, 0);
    }
    // Leftovers of an earlier local gateway mean nothing on a server that has none.
    if !base.local {
        hosts.retain(|h| *h != 0);
    }
    let mut total = SyncReport::default();
    let mut first_error = None;
    for hid in hosts {
        match sync_host(state, hid).await {
            Ok(r) => {
                total.labs += r.labs;
                total.clients += r.clients;
                total.allows += r.allows;
                total.removed_clients += r.removed_clients;
            }
            Err(e) => {
                tracing::warn!("vpn sync of host {hid} failed: {e:?}");
                first_error.get_or_insert(e);
            }
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(total),
    }
}

/// Brings one gateway in line with the database. Safe to call at any time.
pub async fn sync_host(state: &AppState, hid: i64) -> Result<SyncReport, ApiError> {
    let setup = setup_for(state, hid).await?;
    let setup = &setup;
    let _guard = setup.lock.lock().await;
    let material = authority(state, hid).await?;
    let server = server_certificate(state, &material.ca, hid).await?;

    // Static files the gateway reads.
    let dir = &setup.dir;
    write_atomic(&dir.join("pki/ca.crt"), material.ca.cert_pem(), false)?;
    write_atomic(&dir.join("pki/server.crt"), &server.cert_pem, false)?;
    write_atomic(&dir.join("pki/server.key"), &server.key_pem, true)?;
    write_atomic(&dir.join("pki/ta.key"), &material.ta, true)?;
    write_crl(state, setup, &material.ca, hid).await?;
    write_atomic(
        &dir.join("server.conf"),
        &server_conf(&setup.tunnel, &setup.labs.pool()),
        false,
    )?;

    // Which players exist and may connect.
    let mut desired: HashMap<i64, ClientRow> = HashMap::new();
    for client in store::clients(&state.pool, hid).await? {
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
    let labs = store::labs_on(&state.pool, hid).await?;
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

    // The labs must exist before the gateway joins them, and the gateway must run before it can
    // join anything. For a lab host, `ensure_running` also sends the files above across.
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
    if state.vpn_setup().is_none() {
        return Ok(0);
    }
    let now = state.now();
    let mut removed_on: BTreeSet<i64> = BTreeSet::new();
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
        // A lab host that does not answer keeps its labs until it does.
        let Ok(setup) = setup_for(state, lab.lab_host_id).await else {
            continue;
        };
        if setup.gateway.detach(&lab.network).await.is_err() {
            continue;
        }
        if setup.runtime.remove_lab(&lab.network).await.is_err() {
            // Something is still in it; try again on the next pass.
            let _ = setup.gateway.attach(&lab.network).await;
            continue;
        }
        store::delete_lab(&state.pool, lab.id).await?;
        removed_on.insert(lab.lab_host_id);
    }
    for hid in &removed_on {
        if let Err(e) = sync_host(state, *hid).await {
            tracing::warn!("vpn sync of host {hid} failed: {e:?}");
        }
    }
    Ok(removed_on.len())
}
