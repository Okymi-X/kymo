//! The lab host: an authenticated API in front of a container runtime, with the rules that
//! make it safe to leave running on a machine of its own.
//!
//! * It trusts nothing the platform sends: names are checked, images must have a manifest
//!   (unless the operator chose open mode), limits are clamped, environment is allow-listed.
//! * It enforces expiry itself, so a platform that is down or gone cannot leave containers
//!   running for ever.
//! * It remembers what it runs in a state file, so a restart loses nothing, and it can be told
//!   what the platform still wants (`reconcile`) and drop the rest.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use kymo_instances::{
    ExtraPort, InstanceRuntime, InstanceSpec, Placement, ResourceCaps, RunFlag, RuntimeError,
    RuntimeStatus, StartRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::manifest::Manifest;
use crate::naming::valid_name;
use crate::protocol::{
    API_VERSION, ArtifactInfo, CreateRequest, ErrorBody, GatewayState, HealthInfo, InstanceInfo,
    KickBody, LabNetBody, LabPlacement, LogLine, NetworkBody, OkBody, ReadyInfo, ReconcileRequest,
    Removed, VpnFiles,
};
use crate::vm::{VmRuntime, VmStart};
use crate::vpn::LabVpn;

const LOG_LINES: usize = 500;
const AUTH_WINDOW: Duration = Duration::from_secs(60);
const AUTH_MAX_FAILURES: u32 = 10;
const DEFAULT_GRACE_SECS: i64 = 60;
/// How long past its time an instance is left before the reaper removes it.
const EXPIRY_GRACE_SECS: i64 = 3;
/// How long a machine may wait for its flags to be written before it is dropped.
const STUCK_SETUP_SECS: i64 = 600;
/// How long to keep trying to write flags into a machine that gave no boot timeout.
const DEFAULT_BOOT_SECS: u32 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Only artifacts with a manifest run. The default.
    Manifest,
    /// Any valid image runs, within the lab's caps. Convenient on a private network; the
    /// platform can then make the lab host run any image it can pull.
    Open,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Manifest => "manifest",
            Mode::Open => "open",
        }
    }
}

/// An address range, for the source allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    net: IpAddr,
    prefix: u8,
}

impl Cidr {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (addr, prefix) = match text.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (text, None),
        };
        let net: IpAddr = addr
            .parse()
            .map_err(|_| format!("{text} is not an address or range"))?;
        let max = if net.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= max)
                .ok_or_else(|| format!("{text} has a bad prefix length"))?,
            None => max,
        };
        Ok(Self { net, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        fn bits(ip: IpAddr) -> u128 {
            match ip {
                IpAddr::V4(v4) => u128::from(u32::from(v4)),
                IpAddr::V6(v6) => u128::from(v6),
            }
        }
        // An IPv4-mapped IPv6 peer (`::ffff:a.b.c.d`) counts as its IPv4 address.
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 => v4,
        };
        if self.net.is_ipv4() != ip.is_ipv4() {
            return false;
        }
        let width: u32 = if ip.is_ipv4() { 32 } else { 128 };
        let shift = width - u32::from(self.prefix);
        if shift >= 128 {
            return true;
        }
        bits(self.net) >> shift == bits(ip) >> shift
    }
}

pub struct LabConfig {
    /// The lab host's own name, shown in the platform.
    pub name: String,
    pub token: String,
    pub mode: Mode,
    pub max_instances: usize,
    /// Every name must start with this.
    pub name_prefix: String,
    /// Source addresses allowed to call the API. Empty allows any (the token still applies).
    pub allow_from: Vec<Cidr>,
    pub caps: ResourceCaps,
    pub manifests: Vec<Manifest>,
    /// Where the instance list is kept. `None` keeps it in memory only (tests).
    pub state_path: Option<PathBuf>,
    /// `docker` or `podman`, shown by `health`.
    pub runtime_name: String,
    /// The image of the Verifier container started next to a Responder instance.
    pub verifier_image: String,
    /// Where the organizer's Verifier definitions live, one directory per definition.
    pub verifiers_dir: PathBuf,
    /// Wait for a new Verifier to answer before the instance is handed out. Tests that run
    /// against an in memory runtime switch it off.
    pub verifier_health_check: bool,
}

impl LabConfig {
    pub fn new(name: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            token: token.into(),
            mode: Mode::Manifest,
            max_instances: 100,
            name_prefix: "kymo-".into(),
            allow_from: Vec::new(),
            caps: ResourceCaps::default(),
            manifests: Vec::new(),
            state_path: None,
            runtime_name: "docker".into(),
            verifier_image: "localhost/kymo/verifier:1".into(),
            verifiers_dir: PathBuf::from("/etc/kymo-lab/verifiers"),
            verifier_health_check: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    /// Empty while the container is being created.
    handle: String,
    name: String,
    instance_id: String,
    spec: InstanceSpec,
    meta: BTreeMap<String, String>,
    created_at: i64,
    expires_at: i64,
    host_port: u16,
    /// Players are given just its address, every port open on it (see the manifest's `display`).
    #[serde(default)]
    machine: bool,
    /// `docker` or `vm`: which runtime owns it.
    #[serde(default = "container_kind")]
    runtime: String,
    /// More published services, for a virtual machine that offers several.
    #[serde(default)]
    ports: Vec<ExtraPort>,
    #[serde(default)]
    ready_secs: Option<u32>,
    /// The lab network it sits on, so a restart puts it back in the same place.
    #[serde(default)]
    place: Option<LabPlacement>,
    /// The flags the platform generated for this run, kept so a lab side restart can hand out
    /// the same ones. The state file is private to the service user; values never leave it.
    #[serde(default)]
    flags: Vec<RunFlag>,
    /// The flags are still being written into the machine. The instance is reported as
    /// starting until they are, so a player never gets an address for a box without its flag.
    #[serde(default)]
    pending: bool,
    /// The Verifier container started next to this instance, for a Responder.
    #[serde(default)]
    verifier: Option<VerifierRecord>,
}

/// A Verifier container of an instance. The token lets the lab host read its run log; the
/// player never has it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifierRecord {
    handle: String,
    ip: String,
    port: u16,
    token: String,
}

fn container_kind() -> String {
    "docker".into()
}

/// What a request turns into once the manifest and the limits have had their say.
enum Plan {
    Container(InstanceSpec),
    Vm {
        spec: InstanceSpec,
        start: Box<VmStart>,
    },
}

impl Record {
    fn creating(&self) -> bool {
        self.handle.is_empty()
    }
}

#[derive(Default)]
struct Inner {
    records: Vec<Record>,
    log: VecDeque<LogLine>,
}

/// A lab placement that passed the checks.
struct Checked {
    network: String,
    ip: std::net::Ipv4Addr,
    bind: std::net::Ipv4Addr,
}

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct Lab {
    cfg: LabConfig,
    runtime: Arc<dyn InstanceRuntime>,
    vm: Option<Arc<VmRuntime>>,
    inner: Mutex<Inner>,
    started: Instant,
    clock: Clock,
    auth_failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    token_hash: [u8; 32],
    vpn: std::sync::OnceLock<Arc<LabVpn>>,
    vpn_only: std::sync::atomic::AtomicBool,
}

/// Whether a Verifier answers on its address, tried for a while as it starts.
async fn verifier_answers(ip: &str, port: u16) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    for _ in 0..40 {
        if client
            .get(format!("http://{ip}:{port}/healthz"))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    false
}

fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
    })
}

/// What can go wrong, with the status the platform sees.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict(String),
    Full,
    TooManyAttempts,
    Runtime(RuntimeError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "a valid bearer token is required".into(),
            ),
            ApiError::Forbidden => (
                StatusCode::FORBIDDEN,
                "this address is not allowed to use the lab API".into(),
            ),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "instance not found".into()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::Full => (
                StatusCode::SERVICE_UNAVAILABLE,
                "the lab host is full".into(),
            ),
            ApiError::TooManyAttempts => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many failed attempts, try again in a minute".into(),
            ),
            ApiError::Runtime(RuntimeError::Unavailable(m)) => (StatusCode::SERVICE_UNAVAILABLE, m),
            ApiError::Runtime(RuntimeError::Failed(m)) => (StatusCode::BAD_GATEWAY, m),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

impl From<RuntimeError> for ApiError {
    fn from(e: RuntimeError) -> Self {
        ApiError::Runtime(e)
    }
}

impl Lab {
    pub fn new(cfg: LabConfig, runtime: Arc<dyn InstanceRuntime>) -> Arc<Self> {
        Self::with_clock(cfg, runtime, system_clock())
    }

    /// A lab host that can also run virtual machines.
    pub fn with_vm(
        cfg: LabConfig,
        runtime: Arc<dyn InstanceRuntime>,
        vm: Option<Arc<VmRuntime>>,
    ) -> Arc<Self> {
        Self::with_parts(cfg, runtime, vm, system_clock())
    }

    pub fn with_clock(
        cfg: LabConfig,
        runtime: Arc<dyn InstanceRuntime>,
        clock: Clock,
    ) -> Arc<Self> {
        Self::with_parts(cfg, runtime, None, clock)
    }

    pub fn with_parts(
        cfg: LabConfig,
        runtime: Arc<dyn InstanceRuntime>,
        vm: Option<Arc<VmRuntime>>,
        clock: Clock,
    ) -> Arc<Self> {
        let token_hash = Sha256::digest(cfg.token.as_bytes()).into();
        Arc::new(Self {
            cfg,
            runtime,
            vm,
            inner: Mutex::new(Inner::default()),
            started: Instant::now(),
            clock,
            auth_failures: Mutex::default(),
            token_hash,
            vpn: std::sync::OnceLock::new(),
            vpn_only: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    /// The runtime that owns a record.
    fn runtime_of(&self, kind: &str) -> Option<Arc<dyn InstanceRuntime>> {
        match kind {
            "vm" => self.vm.clone().map(|v| v as Arc<dyn InstanceRuntime>),
            _ => Some(Arc::clone(&self.runtime)),
        }
    }

    /// Stops a record's machine or container. A runtime that is gone counts as stopped.
    async fn stop_record(&self, record: &Record) -> Result<(), RuntimeError> {
        // The Verifier goes with its instance, whatever happens to the instance itself.
        if let Some(verifier) = &record.verifier {
            let _ = self.runtime.stop(&verifier.handle).await;
        }
        match self.runtime_of(&record.runtime) {
            Some(rt) if !record.handle.is_empty() => rt.stop(&record.handle).await,
            _ => Ok(()),
        }
    }

    /// Starts unpacking every virtual machine image in the background, so the first player does
    /// not wait for it. Called when the lab host starts.
    pub fn prepare_images(&self) {
        let Some(vm) = &self.vm else { return };
        for m in self.cfg.manifests.iter().filter(|m| m.is_vm()) {
            if let Some(file) = &m.file {
                vm.spawn_prepare(&m.artifact, file);
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn note(&self, action: &str, name: &str, detail: impl Into<String>) {
        let detail = detail.into();
        tracing::info!(action, name, %detail, "lab");
        let mut inner = self.lock();
        if inner.log.len() >= LOG_LINES {
            inner.log.pop_front();
        }
        let at = self.now();
        inner.log.push_back(LogLine {
            at,
            action: action.into(),
            name: name.into(),
            detail,
        });
    }

    fn save(&self, inner: &Inner) {
        let Some(path) = &self.cfg.state_path else {
            return;
        };
        let write = || -> Result<(), String> {
            let json = serde_json::to_vec_pretty(&inner.records).map_err(|e| e.to_string())?;
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, path).map_err(|e| e.to_string())
        };
        if let Err(e) = write() {
            tracing::warn!("could not save the instance list: {e}");
        }
    }

    /// Reads the state file and brings it in line with what the runtimes really run: records
    /// whose container or machine is gone are dropped, ones nobody recorded are adopted so the
    /// next reconcile can decide about them.
    pub async fn load(&self) {
        if let Some(path) = &self.cfg.state_path
            && let Ok(text) = std::fs::read_to_string(path)
        {
            match serde_json::from_str::<Vec<Record>>(&text) {
                Ok(records) => self.lock().records = records,
                Err(e) => tracing::warn!("ignoring an unreadable state file: {e}"),
            }
        }
        let mut managed: Vec<(String, kymo_instances::Managed)> = Vec::new();
        let mut listed = Vec::new();
        for kind in ["docker", "vm"] {
            let Some(rt) = self.runtime_of(kind) else {
                continue;
            };
            if kind == "vm" && self.vm.is_none() {
                continue;
            }
            if let Ok(list) = rt.list_managed().await {
                listed.push(kind);
                managed.extend(list.into_iter().map(|m| (kind.to_owned(), m)));
            }
        }
        let now = self.now();
        let mut inner = self.lock();
        // Only records of a runtime that answered are judged; one that could not be asked says
        // nothing about its containers.
        inner.records.retain(|r| {
            !listed.contains(&r.runtime.as_str())
                || managed
                    .iter()
                    .any(|(k, m)| *k == r.runtime && m.handle == r.handle)
        });
        // A start that was in progress when the process died has no container to wait for.
        inner.records.retain(|r| !r.creating());
        for (kind, m) in &managed {
            if !inner
                .records
                .iter()
                .any(|r| r.runtime == *kind && r.handle == m.handle)
            {
                inner.records.push(Record {
                    handle: m.handle.clone(),
                    name: if kind == "vm" && valid_name(&m.handle) {
                        m.handle.clone()
                    } else {
                        format!(
                            "{}adopted-{}",
                            self.cfg.name_prefix,
                            &m.handle[..m.handle.len().min(8)]
                        )
                    },
                    instance_id: m.instance_id.clone(),
                    spec: InstanceSpec {
                        image: "unknown".into(),
                        port: 1,
                        cpu_millis: 1,
                        memory_mb: 1,
                        pids: 1,
                        ttl_secs: 1,
                        env: vec![],
                        read_only: true,
                    },
                    meta: BTreeMap::new(),
                    created_at: now,
                    expires_at: now + DEFAULT_GRACE_SECS * 10,
                    host_port: 0,
                    runtime: kind.clone(),
                    ports: Vec::new(),
                    ready_secs: None,
                    place: None,
                    flags: Vec::new(),
                    pending: false,
                    verifier: None,
                    machine: false,
                });
            }
        }
        self.save(&inner);
        drop(inner);
        if let Some(vm) = &self.vm {
            vm.revive_net();
        }
    }

    fn find(&self, id: &str) -> Option<Record> {
        let inner = self.lock();
        inner
            .records
            .iter()
            .find(|r| r.name == id)
            .or_else(|| {
                inner
                    .records
                    .iter()
                    .find(|r| !r.handle.is_empty() && r.handle == id)
            })
            .or_else(|| inner.records.iter().find(|r| r.instance_id == id))
            .cloned()
    }

    /// Where the instance and its Verifier can be reached on the lab network.
    fn endpoints_of(&self, record: &Record) -> Vec<kymo_instances::Endpoint> {
        let Some(place) = &record.place else {
            return Vec::new();
        };
        let mut out = vec![kymo_instances::Endpoint {
            role: "machine".into(),
            name: "Machine".into(),
            host: place.ip.clone(),
            port: None,
            scheme: None,
        }];
        if let Some(v) = &record.verifier {
            out.push(kymo_instances::Endpoint {
                role: "verifier".into(),
                name: "Verifier".into(),
                host: v.ip.clone(),
                port: Some(v.port),
                scheme: Some("http".into()),
            });
        }
        out
    }

    async fn info(&self, record: &Record) -> InstanceInfo {
        let status = if record.creating() {
            "creating"
        } else if record.pending {
            "starting"
        } else {
            match self.runtime_of(&record.runtime) {
                None => "unknown",
                Some(rt) => match rt.status(&record.handle).await {
                    Ok(RuntimeStatus::Running) => "running",
                    Ok(RuntimeStatus::Exited) => "exited",
                    Ok(RuntimeStatus::Missing) => "missing",
                    Err(_) => "unknown",
                },
            }
        };
        InstanceInfo {
            id: record.handle.clone(),
            name: record.name.clone(),
            instance_id: record.instance_id.clone(),
            status: status.into(),
            host_port: record.host_port,
            ports: record.ports.clone(),
            runtime: record.runtime.clone(),
            ready_secs: record.ready_secs,
            machine: record.machine,
            created_at: record.created_at,
            expires_at: record.expires_at,
            meta: record.meta.clone(),
            ip: record.place.as_ref().map(|p| p.ip.clone()),
            endpoints: self.endpoints_of(record),
        }
    }

    /// Decides what to run for a request, or refuses it.
    fn plan(&self, request: &CreateRequest) -> Result<Plan, ApiError> {
        if !valid_name(&request.name) || !request.name.starts_with(&self.cfg.name_prefix) {
            return Err(ApiError::BadRequest(format!(
                "the name must be lowercase letters, digits and dashes, 3 to 63 characters, starting with {}",
                self.cfg.name_prefix
            )));
        }
        if request.instance_id.is_empty()
            || request.instance_id.len() > 64
            || !request
                .instance_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ApiError::BadRequest("the instance id is not valid".into()));
        }
        if request.meta.len() > 16
            || request
                .meta
                .iter()
                .any(|(k, v)| k.len() > 32 || v.len() > 120)
        {
            return Err(ApiError::BadRequest("the metadata is too large".into()));
        }
        match self.cfg.mode {
            Mode::Manifest => {
                let manifest = self
                    .cfg
                    .manifests
                    .iter()
                    .find(|m| m.artifact == request.spec.image)
                    .ok_or_else(|| {
                        ApiError::BadRequest(format!(
                            "this lab host has no manifest for {}, so it will not run it",
                            request.spec.image
                        ))
                    })?;
                let spec = manifest
                    .resolve(&request.spec, &self.cfg.caps)
                    .map_err(|e| ApiError::BadRequest(e.0))?;
                if !manifest.is_vm() {
                    return Ok(Plan::Container(spec));
                }
                let Some(vm) = &self.vm else {
                    return Err(ApiError::Runtime(RuntimeError::Unavailable(
                        "this lab host is not set up for virtual machines (QEMU is missing or switched off)".into(),
                    )));
                };
                // The image unpacks in the background the first time; until it is ready a start
                // is "try again shortly" and creates nothing.
                if let Some(file) = &manifest.file {
                    vm.spawn_prepare(&manifest.artifact, file);
                }
                if let (false, note) = vm.prepared(&manifest.artifact) {
                    return Err(ApiError::Runtime(RuntimeError::Unavailable(format!(
                        "the image {} is not ready: {}",
                        manifest.artifact,
                        note.unwrap_or_default()
                    ))));
                }
                let start = VmStart::from_manifest(
                    manifest,
                    &request.instance_id,
                    &request.name,
                    spec.memory_mb,
                    spec.cpu_millis,
                );
                Ok(Plan::Vm {
                    spec,
                    start: Box::new(start),
                })
            }
            Mode::Open => {
                let mut spec = request.spec.clone();
                spec.read_only = true;
                spec.validate(&self.cfg.caps)
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                Ok(Plan::Container(spec))
            }
        }
    }

    /// The VPN gateway of this lab host, when it has one.
    pub fn vpn(&self) -> Option<Arc<LabVpn>> {
        self.vpn.get().cloned()
    }

    /// Turns the VPN on. `only` refuses every instance that is not on a lab network, so nothing
    /// can be published on the host by mistake.
    pub fn set_vpn(&self, vpn: Arc<LabVpn>, only: bool) {
        let _ = self.vpn.set(vpn);
        self.vpn_only
            .store(only, std::sync::atomic::Ordering::Relaxed);
    }

    fn is_vpn_only(&self) -> bool {
        self.vpn_only.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Checks where the request wants its instance, and refuses what this lab host should not do.
    fn check_placement(&self, request: &CreateRequest) -> Result<Option<Checked>, ApiError> {
        let Some(place) = &request.lab else {
            if self.is_vpn_only() {
                return Err(ApiError::BadRequest(
                    "this lab host only runs instances on a private VPN network, never on a published port".into(),
                ));
            }
            return Ok(None);
        };
        let vpn = self
            .vpn()
            .ok_or_else(|| ApiError::BadRequest("this lab host has no VPN gateway".into()))?;
        if !LabVpn::valid_network(&place.network) {
            return Err(ApiError::BadRequest("the lab network is not valid".into()));
        }
        let ip: std::net::Ipv4Addr = place
            .ip
            .parse()
            .map_err(|_| ApiError::BadRequest("the lab address is not valid".into()))?;
        let bind: std::net::Ipv4Addr = place
            .bind
            .parse()
            .map_err(|_| ApiError::BadRequest("the lab bind address is not valid".into()))?;
        if !vpn.in_pool(ip) || !vpn.in_pool(bind) {
            return Err(ApiError::BadRequest(
                "the lab address is outside this lab host's VPN range".into(),
            ));
        }
        if let Some(verifier) = &place.verifier_ip {
            let valid = verifier
                .parse::<std::net::Ipv4Addr>()
                .is_ok_and(|v| vpn.in_pool(v) && v != ip);
            if !valid {
                return Err(ApiError::BadRequest(
                    "the Verifier address is not valid, or is the machine's own".into(),
                ));
            }
        }
        Ok(Some(Checked {
            network: place.network.clone(),
            ip,
            bind,
        }))
    }

    pub async fn create(
        self: &Arc<Self>,
        request: CreateRequest,
    ) -> Result<InstanceInfo, ApiError> {
        let mut plan = self.plan(&request)?;
        let lab = self.check_placement(&request)?;
        let manifest = self
            .cfg
            .manifests
            .iter()
            .find(|m| m.artifact == request.spec.image)
            .cloned();
        // Where the run's flags go, and whether a Verifier starts. A flag the manifest has no
        // place for is refused, so a typo cannot start an instance without its flag.
        let flag_plan = match &manifest {
            Some(m) => crate::inject::plan_flags(
                &request.flags,
                m.inject.as_ref(),
                m.verifier.as_ref(),
                m.is_vm(),
                request.lab.as_ref(),
            )
            .map_err(|e| ApiError::BadRequest(e.0))?,
            None if request.flags.is_empty()
                && request.lab.as_ref().is_none_or(|l| l.verifier_ip.is_none()) =>
            {
                crate::inject::FlagPlan::default()
            }
            None => {
                return Err(ApiError::BadRequest(
                    "flags and a Verifier need a manifest for this image".into(),
                ));
            }
        };
        if let (Plan::Vm { start, .. }, Some(l)) = (&mut plan, &lab) {
            // On a lab network the machine is a machine of its own: a tap on the network's bridge
            // and the address the platform reserved, with every port open on it.
            start.tap = Some(crate::tapnet::TapSpec {
                ip: l.ip,
                gateway: l.bind,
                prefix: 0,
            });
        }
        let machine = self
            .cfg
            .manifests
            .iter()
            .find(|m| m.artifact == request.spec.image)
            .is_some_and(|m| m.machine_style());
        let (spec, kind) = match &plan {
            Plan::Container(spec) => (spec.clone(), "docker"),
            Plan::Vm { spec, .. } => (spec.clone(), "vm"),
        };
        let now = self.now();
        // Reserve the name and a slot before the slow part, so two requests cannot both pass.
        // The lock is released before anything is awaited.
        let reservation = {
            let mut inner = self.lock();
            if let Some(existing) = inner.records.iter().find(|r| r.name == request.name) {
                // The platform retrying a create it did not see the answer to.
                Err((
                    existing.clone(),
                    existing.instance_id == request.instance_id,
                ))
            } else if inner
                .records
                .iter()
                .any(|r| r.instance_id == request.instance_id)
            {
                return Err(ApiError::Conflict(
                    "an instance with this id already exists".into(),
                ));
            } else if inner.records.len() >= self.cfg.max_instances {
                return Err(ApiError::Full);
            } else {
                inner.records.push(Record {
                    handle: String::new(),
                    name: request.name.clone(),
                    instance_id: request.instance_id.clone(),
                    spec: spec.clone(),
                    meta: request.meta.clone(),
                    created_at: now,
                    expires_at: now + spec.ttl_secs,
                    host_port: 0,
                    runtime: kind.to_owned(),
                    ports: Vec::new(),
                    ready_secs: None,
                    machine,
                    place: request.lab.clone(),
                    flags: request.flags.clone(),
                    pending: false,
                    verifier: None,
                });
                Ok(())
            }
        };
        if let Err((existing, same)) = reservation {
            return if same && !existing.creating() {
                Ok(self.info(&existing).await)
            } else {
                Err(ApiError::Conflict(format!(
                    "the name {} is already in use",
                    request.name
                )))
            };
        }
        let outcome = match (&plan, &self.vm) {
            (Plan::Vm { start, .. }, Some(vm)) => vm.start_vm(start, now).await,
            (Plan::Vm { .. }, None) => Err(RuntimeError::Unavailable(
                "no virtual machine runtime".into(),
            )),
            (Plan::Container(spec), _) => {
                // The flag variables are added only for this start. The stored spec stays what the
                // platform asked for, so a restart can resolve it against the manifest again.
                let mut started_spec = spec.clone();
                started_spec.env.extend(flag_plan.env.iter().cloned());
                let start = StartRequest {
                    instance_id: request.instance_id.clone(),
                    spec: started_spec,
                    placement: match &lab {
                        Some(l) => Placement::Lab {
                            network: l.network.clone(),
                            ip: l.ip.to_string(),
                            gateway: Some(l.bind.to_string()),
                        },
                        None => Placement::Published,
                    },
                    name: Some(request.name.clone()),
                    meta: request
                        .meta
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                };
                self.runtime.start(&start).await
            }
        };
        match outcome {
            Ok(started) => {
                let updated = {
                    let mut inner = self.lock();
                    let found = inner
                        .records
                        .iter_mut()
                        .find(|r| r.name == request.name)
                        .map(|record| {
                            record.handle = started.handle.clone();
                            record.host_port = started.host_port;
                            record.ports = started.extra_ports.clone();
                            record.ready_secs = started.ready_secs;
                            record.pending = !flag_plan.files.is_empty();
                            record.clone()
                        });
                    if found.is_some() {
                        self.save(&inner);
                    }
                    found
                };
                let Some(record) = updated else {
                    // Removed while starting; do not leave it behind.
                    if let Some(rt) = self.runtime_of(kind) {
                        let _ = rt.stop(&started.handle).await;
                    }
                    return Err(ApiError::NotFound);
                };
                self.note(
                    "create",
                    &request.name,
                    format!("{} ({kind}) for {}", spec.image, meta_text(&request.meta)),
                );
                if flag_plan.verifier
                    && let Err(e) = self
                        .attach_verifier(&request, manifest.as_ref(), &flag_plan, &record)
                        .await
                {
                    // No machine without its Verifier: take everything back.
                    let _ = self.remove(&request.name).await;
                    self.note(
                        "create-failed",
                        &request.name,
                        "the Verifier could not be started",
                    );
                    return Err(e);
                }
                // Read the record again: the Verifier was added to it.
                let record = self.find(&request.name).unwrap_or(record);
                if !flag_plan.files.is_empty() {
                    let (host, port) = self.ssh_endpoint(
                        &request,
                        manifest.as_ref().and_then(|m| m.inject.as_ref()),
                        &started,
                    );
                    self.spawn_inject(
                        record.name.clone(),
                        manifest.as_ref().and_then(|m| m.inject.clone()),
                        flag_plan.files.clone(),
                        host,
                        port,
                        started.ready_secs,
                    );
                }
                Ok(self.info(&record).await)
            }
            Err(e) => {
                {
                    let mut inner = self.lock();
                    inner.records.retain(|r| r.name != request.name);
                    self.save(&inner);
                }
                self.note("create-failed", &request.name, e.to_string());
                Err(e.into())
            }
        }
    }

    /// Starts the Verifier container of a Responder instance on the instance's lab network and
    /// waits for it to answer. The held flag goes to it and nowhere else.
    async fn attach_verifier(
        &self,
        request: &CreateRequest,
        manifest: Option<&Manifest>,
        plan: &crate::inject::FlagPlan,
        record: &Record,
    ) -> Result<(), ApiError> {
        let bad = |m: &str| ApiError::BadRequest(m.into());
        let (Some(block), Some(lab)) = (
            manifest.and_then(|m| m.verifier.as_ref()),
            request.lab.as_ref(),
        ) else {
            return Err(bad("a Verifier needs a manifest block and a lab network"));
        };
        let ip = lab
            .verifier_ip
            .clone()
            .ok_or_else(|| bad("the platform gave no Verifier address"))?;
        let dir = self.cfg.verifiers_dir.join(&block.definition);
        if !dir.join("verifier.json").is_file() {
            return Err(bad("the Verifier definition for this image is missing"));
        }
        let name = format!("{}-v", request.name);
        if name.len() > 64 {
            return Err(bad("the instance name is too long for a Verifier"));
        }
        let ports: Vec<u16> = manifest
            .map(|m| m.declared_ports().iter().map(|p| p.port).collect())
            .unwrap_or_default();
        let token: String = (0..16)
            .map(|_| format!("{:02x}", rand::random::<u8>()))
            .collect();
        let run_json = serde_json::json!({
            "target_ip": lab.ip,
            "target_ports": ports,
            "flag": plan.held.first().map(|f| f.value.clone()),
            "expires_at": record.expires_at,
            "admin_token": token,
        })
        .to_string();
        let spec = kymo_instances::VerifierSpec {
            name,
            instance_id: request.instance_id.clone(),
            network: lab.network.clone(),
            ip: ip.clone(),
            image: self.cfg.verifier_image.clone(),
            port: block.port,
            memory_mb: block.memory_mb(),
            cpu_millis: block.cpu_millis(),
            definition_dir: dir.to_string_lossy().into_owned(),
            run_json,
        };
        let handle = self.runtime.start_verifier(&spec).await?;
        {
            let mut inner = self.lock();
            if let Some(r) = inner.records.iter_mut().find(|r| r.name == request.name) {
                r.verifier = Some(VerifierRecord {
                    handle: handle.clone(),
                    ip: ip.clone(),
                    port: block.port,
                    token,
                });
            }
            self.save(&inner);
        }
        if self.cfg.verifier_health_check && !verifier_answers(&ip, block.port).await {
            return Err(ApiError::Runtime(RuntimeError::Failed(
                "the Verifier did not start".into(),
            )));
        }
        Ok(())
    }

    /// Where the lab host reaches the guest's SSH: its address on the lab network, or the port
    /// the guest's SSH was forwarded to on this machine.
    fn ssh_endpoint(
        &self,
        request: &CreateRequest,
        inject: Option<&crate::inject::Inject>,
        started: &kymo_instances::Started,
    ) -> (String, u16) {
        let guest_port = inject.map_or(22, |i| i.port);
        match &request.lab {
            Some(lab) => (lab.ip.clone(), guest_port),
            None => {
                let mapped = started
                    .extra_ports
                    .iter()
                    .find(|p| p.guest == guest_port)
                    .map_or(started.host_port, |p| p.host);
                ("127.0.0.1".to_owned(), mapped)
            }
        }
    }

    /// Writes the flags into a machine in the background, then lets the instance report as
    /// running. If it cannot be done in time the instance is removed, so no player ever gets a
    /// machine without its flag. Neither the log nor the record ever carries a flag value.
    fn spawn_inject(
        self: &Arc<Self>,
        name: String,
        inject: Option<crate::inject::Inject>,
        files: Vec<crate::inject::FileWrite>,
        host: String,
        port: u16,
        boot_secs: Option<u32>,
    ) {
        let lab = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = match inject {
                Some(inject) => {
                    let target = crate::ssh::SshTarget {
                        host,
                        port,
                        user: inject.user.clone().unwrap_or_default(),
                        key: inject.ssh_key.clone().map(std::path::PathBuf::from),
                        password: inject.ssh_password.clone(),
                        sudo: inject.sudo,
                    };
                    let patience = std::time::Duration::from_secs(u64::from(
                        boot_secs.unwrap_or(DEFAULT_BOOT_SECS),
                    ));
                    crate::ssh::write_files(&target, &files, patience)
                        .await
                        .map_err(|e| e.to_string())
                }
                None => Err("the manifest has no inject block".to_owned()),
            };
            match outcome {
                Ok(()) => {
                    {
                        let mut inner = lab.lock();
                        if let Some(r) = inner.records.iter_mut().find(|r| r.name == name) {
                            r.pending = false;
                        }
                        lab.save(&inner);
                    }
                    lab.note("flags-written", &name, format!("{} file(s)", files.len()));
                }
                Err(why) => {
                    lab.note("inject-failed", &name, why);
                    let _ = lab.remove(&name).await;
                }
            }
        });
    }

    /// The run log of an instance's Verifier, for the organizer's Instances page.
    pub async fn verify_log(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        let record = self.find(id).ok_or(ApiError::NotFound)?;
        let v = record.verifier.ok_or(ApiError::NotFound)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|_| ApiError::Runtime(RuntimeError::Failed("no HTTP client".into())))?;
        let answer = client
            .get(format!("http://{}:{}/_admin/log", v.ip, v.port))
            .bearer_auth(&v.token)
            .send()
            .await
            .map_err(|_| {
                ApiError::Runtime(RuntimeError::Unavailable(
                    "the Verifier does not answer".into(),
                ))
            })?;
        if !answer.status().is_success() {
            return Err(ApiError::Runtime(RuntimeError::Unavailable(
                "the Verifier refused the request".into(),
            )));
        }
        answer
            .json()
            .await
            .map_err(|_| ApiError::Runtime(RuntimeError::Failed("the Verifier sent no log".into())))
    }

    pub async fn remove(&self, id: &str) -> Result<(), ApiError> {
        let Some(record) = self.find(id) else {
            return Ok(());
        };
        self.stop_record(&record).await?;
        let mut inner = self.lock();
        inner.records.retain(|r| r.name != record.name);
        self.save(&inner);
        drop(inner);
        self.note("remove", &record.name, "");
        Ok(())
    }

    pub async fn restart(self: &Arc<Self>, id: &str) -> Result<InstanceInfo, ApiError> {
        let record = self.find(id).ok_or(ApiError::NotFound)?;
        if record.creating() {
            return Err(ApiError::Conflict(
                "the instance is still being created".into(),
            ));
        }
        self.stop_record(&record).await?;
        {
            let mut inner = self.lock();
            inner.records.retain(|r| r.name != record.name);
        }
        let request = CreateRequest {
            name: record.name.clone(),
            instance_id: record.instance_id.clone(),
            spec: record.spec.clone(),
            meta: record.meta.clone(),
            lab: record.place.clone(),
            flags: record.flags.clone(),
        };
        let info = self.create(request).await?;
        self.note("restart", &record.name, "");
        Ok(info)
    }

    /// Removes what is past its time. Returns the names.
    pub async fn cleanup(&self) -> Vec<String> {
        let now = self.now();
        let due: Vec<Record> = self
            .lock()
            .records
            .iter()
            .filter(|r| {
                !r.creating()
                    && (now >= r.expires_at + EXPIRY_GRACE_SECS
                        // A machine whose flags were never written (the lab host restarted in
                        // between) is not handed to anyone.
                        || (r.pending && now - r.created_at > STUCK_SETUP_SECS))
            })
            .cloned()
            .collect();
        let mut removed = Vec::new();
        for record in due {
            if self.stop_record(&record).await.is_ok() {
                let mut inner = self.lock();
                inner.records.retain(|r| r.name != record.name);
                self.save(&inner);
                drop(inner);
                self.note("expire", &record.name, "time is up");
                removed.push(record.name);
            }
        }
        // A record whose container or machine disappeared (removed by hand, crashed host) is
        // forgotten.
        let records: Vec<Record> = self
            .lock()
            .records
            .iter()
            .filter(|r| !r.creating())
            .cloned()
            .collect();
        for record in records {
            let Some(rt) = self.runtime_of(&record.runtime) else {
                continue;
            };
            if matches!(rt.status(&record.handle).await, Ok(RuntimeStatus::Missing)) {
                let mut inner = self.lock();
                inner.records.retain(|r| r.name != record.name);
                self.save(&inner);
            }
        }
        // A Verifier whose instance is gone (a crash between the two) is removed.
        if let Ok(found) = self.runtime.list_verifiers().await {
            let known: Vec<String> = self
                .lock()
                .records
                .iter()
                .map(|r| r.instance_id.clone())
                .collect();
            for v in found
                .into_iter()
                .filter(|v| !known.contains(&v.instance_id))
            {
                let _ = self.runtime.stop(&v.handle).await;
            }
        }
        removed
    }

    /// The platform is the source of truth for what should run: removes what it does not list.
    pub async fn reconcile(&self, keep: &[String], grace_secs: i64) -> Vec<String> {
        let now = self.now();
        let orphans: Vec<Record> = self
            .lock()
            .records
            .iter()
            .filter(|r| !keep.contains(&r.instance_id) && now - r.created_at >= grace_secs)
            .cloned()
            .collect();
        let mut removed = Vec::new();
        for record in orphans {
            let ok = record.creating() || self.stop_record(&record).await.is_ok();
            if ok {
                let mut inner = self.lock();
                inner.records.retain(|r| r.name != record.name);
                self.save(&inner);
                drop(inner);
                self.note("reconcile", &record.name, "the platform no longer lists it");
                removed.push(record.name);
            }
        }
        // Containers and machines the runtimes have that this lab never recorded and the platform
        // does not list.
        for kind in ["docker", "vm"] {
            let Some(rt) = self.runtime_of(kind) else {
                continue;
            };
            if kind == "vm" && self.vm.is_none() {
                continue;
            }
            let Ok(managed) = rt.list_managed().await else {
                continue;
            };
            let known: Vec<String> = self
                .lock()
                .records
                .iter()
                .filter(|r| r.runtime == kind)
                .map(|r| r.handle.clone())
                .collect();
            for m in managed {
                if !known.contains(&m.handle)
                    && !keep.contains(&m.instance_id)
                    && rt.stop(&m.handle).await.is_ok()
                {
                    self.note(
                        "reconcile",
                        &m.handle,
                        "an unrecorded container or machine was removed",
                    );
                    removed.push(m.handle);
                }
            }
        }
        removed
    }

    /// The reaper: enforces expiry on this host whatever the platform does.
    pub fn spawn_reaper(self: &Arc<Self>) {
        let lab = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            loop {
                tick.tick().await;
                lab.cleanup().await;
            }
        });
    }

    /// What this lab host can run: containers when a container runtime was set up, machines when
    /// QEMU was.
    fn runtime_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if !self.cfg.runtime_name.is_empty() {
            names.push(self.cfg.runtime_name.clone());
        }
        if self.vm.is_some() {
            names.push("vm".to_owned());
        }
        names
    }

    fn client_check(&self, ip: IpAddr, token: Option<&str>) -> Result<(), ApiError> {
        if !self.cfg.allow_from.is_empty() && !self.cfg.allow_from.iter().any(|c| c.contains(ip)) {
            return Err(ApiError::Forbidden);
        }
        let mut failures = self.auth_failures.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((count, since)) = failures.get(&ip).copied() {
            if since.elapsed() > AUTH_WINDOW {
                failures.remove(&ip);
            } else if count >= AUTH_MAX_FAILURES {
                return Err(ApiError::TooManyAttempts);
            }
        }
        // Hash both sides, so the comparison takes the same time whatever was sent.
        let given: [u8; 32] = Sha256::digest(token.unwrap_or("").as_bytes()).into();
        if token.is_some() && bool::from(given.ct_eq(&self.token_hash)) {
            failures.remove(&ip);
            return Ok(());
        }
        if failures.len() > 10_000 {
            failures.clear();
        }
        let entry = failures.entry(ip).or_insert((0, Instant::now()));
        entry.0 += 1;
        Err(ApiError::Unauthorized)
    }
}

fn meta_text(meta: &BTreeMap<String, String>) -> String {
    meta.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn auth(
    State(lab): State<Arc<Lab>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::from([127, 0, 0, 1]), |c| c.0.ip());
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    lab.client_check(ip, token)?;
    Ok(next.run(request).await)
}

async fn health(State(lab): State<Arc<Lab>>) -> Json<HealthInfo> {
    Json(HealthInfo {
        name: lab.cfg.name.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        api: API_VERSION.into(),
        runtime: lab.cfg.runtime_name.clone(),
        mode: lab.cfg.mode.as_str().into(),
        running: lab.lock().records.len(),
        max_instances: lab.cfg.max_instances,
        artifacts: lab.cfg.manifests.len(),
        uptime_secs: i64::try_from(lab.started.elapsed().as_secs()).unwrap_or(i64::MAX),
        now: lab.now(),
        runtimes: lab.runtime_names(),
        accel: lab.vm.as_ref().and_then(|v| v.accel().ok()).map(|a| {
            if a == crate::vm::Accel::Kvm {
                "kvm"
            } else {
                "tcg"
            }
            .to_owned()
        }),
        vpn: lab.vpn().map(|v| v.info()),
        vpn_only: lab.is_vpn_only(),
    })
}

async fn artifacts(State(lab): State<Arc<Lab>>) -> Json<Vec<ArtifactInfo>> {
    Json(
        lab.cfg
            .manifests
            .iter()
            .map(|m| {
                let (ready, note) = if m.is_vm() {
                    match &lab.vm {
                        Some(vm) => vm.prepared(&m.artifact),
                        None => (
                            false,
                            Some("this lab host is not set up for virtual machines".into()),
                        ),
                    }
                } else {
                    (true, None)
                };
                ArtifactInfo {
                    artifact: m.artifact.clone(),
                    runtime: m.runtime.clone(),
                    port: m.primary_port(),
                    max_ttl_secs: m.max_ttl_secs.unwrap_or(lab.cfg.caps.max_ttl_secs),
                    env_allow: m.env_allow.clone(),
                    ports: m.declared_ports(),
                    ready,
                    note,
                }
            })
            .collect(),
    )
}

async fn log(State(lab): State<Arc<Lab>>) -> Json<Vec<LogLine>> {
    Json(lab.lock().log.iter().rev().cloned().collect())
}

async fn create(
    State(lab): State<Arc<Lab>>,
    Json(request): Json<CreateRequest>,
) -> Result<(StatusCode, Json<InstanceInfo>), ApiError> {
    Ok((StatusCode::CREATED, Json(lab.create(request).await?)))
}

async fn list(State(lab): State<Arc<Lab>>) -> Json<Vec<InstanceInfo>> {
    let records: Vec<Record> = lab.lock().records.clone();
    let mut out = Vec::with_capacity(records.len());
    for record in &records {
        out.push(lab.info(record).await);
    }
    Json(out)
}

async fn get_one(
    State(lab): State<Arc<Lab>>,
    Path(id): Path<String>,
) -> Result<Json<InstanceInfo>, ApiError> {
    let record = lab.find(&id).ok_or(ApiError::NotFound)?;
    Ok(Json(lab.info(&record).await))
}

async fn verify_log(
    State(lab): State<Arc<Lab>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(lab.verify_log(&id).await?))
}

async fn remove(
    State(lab): State<Arc<Lab>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    lab.remove(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restart(
    State(lab): State<Arc<Lab>>,
    Path(id): Path<String>,
) -> Result<Json<InstanceInfo>, ApiError> {
    Ok(Json(lab.restart(&id).await?))
}

async fn reconcile(
    State(lab): State<Arc<Lab>>,
    Json(request): Json<ReconcileRequest>,
) -> Json<Removed> {
    let grace = request.grace_secs.unwrap_or(DEFAULT_GRACE_SECS).max(0);
    Json(Removed {
        removed: lab.reconcile(&request.keep, grace).await,
    })
}

async fn cleanup(State(lab): State<Arc<Lab>>) -> Json<Removed> {
    Json(Removed {
        removed: lab.cleanup().await,
    })
}

/// The console of a virtual machine, as a PNG.
async fn screen(State(lab): State<Arc<Lab>>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let record = lab.find(&id).ok_or(ApiError::NotFound)?;
    let vm = match (&lab.vm, record.runtime.as_str()) {
        (Some(vm), "vm") if !record.creating() => vm,
        _ => {
            return Err(ApiError::BadRequest(
                "only a running virtual machine has a screen".into(),
            ));
        }
    };
    let png = vm.screenshot(&record.handle).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png,
    )
        .into_response())
}

#[derive(Deserialize)]
struct ReadyQuery {
    port: u16,
}

/// Whether the service behind a published port answers. A virtual machine is asked about through
/// QEMU; a container is up when its port is, which the platform checks by connecting.
async fn ready(
    State(lab): State<Arc<Lab>>,
    axum::extract::Query(query): axum::extract::Query<ReadyQuery>,
) -> Json<ReadyInfo> {
    let found = {
        let inner = lab.lock();
        inner.records.iter().find_map(|r| {
            if r.creating() {
                return None;
            }
            if r.host_port == query.port {
                return Some((r.clone(), r.spec.port));
            }
            r.ports
                .iter()
                .find(|p| p.host == query.port)
                .map(|p| (r.clone(), p.guest))
        })
    };
    let Some((record, guest)) = found else {
        // Not one of this lab host's instances: no opinion, the caller's own check decides.
        return Json(ReadyInfo {
            ready: true,
            vouched: false,
        });
    };
    match (&lab.vm, record.runtime.as_str()) {
        (Some(vm), "vm") => Json(ReadyInfo {
            ready: vm.guest_ready(&record.handle, query.port, guest).await,
            vouched: true,
        }),
        _ => Json(ReadyInfo {
            ready: true,
            vouched: false,
        }),
    }
}

// ---------- the lab host's VPN gateway ----------

fn gateway_error(e: kymo_vpn::GatewayError) -> ApiError {
    match e {
        kymo_vpn::GatewayError::Unavailable(m) => ApiError::Runtime(RuntimeError::Unavailable(m)),
        other => ApiError::Runtime(RuntimeError::Failed(other.to_string())),
    }
}

fn vpn_of(lab: &Lab) -> Result<Arc<LabVpn>, ApiError> {
    lab.vpn()
        .ok_or_else(|| ApiError::BadRequest("this lab host has no VPN gateway".into()))
}

async fn vpn_files(
    State(lab): State<Arc<Lab>>,
    Json(files): Json<VpnFiles>,
) -> Result<Json<OkBody>, ApiError> {
    let vpn = vpn_of(&lab)?;
    vpn.write_files(&files).map_err(ApiError::BadRequest)?;
    Ok(Json(OkBody { ok: true }))
}

async fn vpn_ensure(State(lab): State<Arc<Lab>>) -> Result<Json<GatewayState>, ApiError> {
    let vpn = vpn_of(&lab)?;
    vpn.ensure().await.map_err(gateway_error)?;
    Ok(Json(vpn.status().await.map_err(gateway_error)?))
}

async fn vpn_state(State(lab): State<Arc<Lab>>) -> Result<Json<GatewayState>, ApiError> {
    let vpn = vpn_of(&lab)?;
    Ok(Json(vpn.status().await.map_err(gateway_error)?))
}

async fn vpn_attach(
    State(lab): State<Arc<Lab>>,
    Json(body): Json<NetworkBody>,
) -> Result<Json<OkBody>, ApiError> {
    let vpn = vpn_of(&lab)?;
    if !LabVpn::valid_network(&body.network) {
        return Err(ApiError::BadRequest("the lab network is not valid".into()));
    }
    vpn.attach(&body.network).await.map_err(gateway_error)?;
    Ok(Json(OkBody { ok: true }))
}

async fn vpn_detach(
    State(lab): State<Arc<Lab>>,
    Json(body): Json<NetworkBody>,
) -> Result<Json<OkBody>, ApiError> {
    let vpn = vpn_of(&lab)?;
    if !LabVpn::valid_network(&body.network) {
        return Err(ApiError::BadRequest("the lab network is not valid".into()));
    }
    vpn.detach(&body.network).await.map_err(gateway_error)?;
    Ok(Json(OkBody { ok: true }))
}

async fn vpn_rules(State(lab): State<Arc<Lab>>) -> Result<Json<OkBody>, ApiError> {
    vpn_of(&lab)?.rules().await.map_err(gateway_error)?;
    Ok(Json(OkBody { ok: true }))
}

async fn vpn_kick(
    State(lab): State<Arc<Lab>>,
    Json(body): Json<KickBody>,
) -> Result<Json<OkBody>, ApiError> {
    if !kymo_vpn::valid_common_name(&body.common_name) {
        return Err(ApiError::BadRequest("the player name is not valid".into()));
    }
    vpn_of(&lab)?
        .kick(&body.common_name)
        .await
        .map_err(gateway_error)?;
    Ok(Json(OkBody { ok: true }))
}

#[derive(Deserialize)]
struct ReachQuery {
    ip: String,
    port: u16,
}

async fn vpn_reachable(
    State(lab): State<Arc<Lab>>,
    axum::extract::Query(q): axum::extract::Query<ReachQuery>,
) -> Result<Json<OkBody>, ApiError> {
    let vpn = vpn_of(&lab)?;
    let ip: std::net::Ipv4Addr =
        q.ip.parse()
            .map_err(|_| ApiError::BadRequest("the address is not valid".into()))?;
    // Only addresses of lab networks: this must not become a port scanner for the lab host's
    // own network.
    if !vpn.in_pool(ip) {
        return Err(ApiError::BadRequest(
            "the address is outside this lab host's VPN range".into(),
        ));
    }
    let ok = vpn.reachable(ip, q.port).await.map_err(gateway_error)?;
    Ok(Json(OkBody { ok }))
}

async fn vpn_lab_put(
    State(lab): State<Arc<Lab>>,
    Path(network): Path<String>,
    Json(body): Json<LabNetBody>,
) -> Result<Json<OkBody>, ApiError> {
    vpn_of(&lab)?
        .ensure_lab(&network, &body.subnet)
        .await
        .map_err(ApiError::BadRequest)?;
    Ok(Json(OkBody { ok: true }))
}

async fn vpn_lab_delete(
    State(lab): State<Arc<Lab>>,
    Path(network): Path<String>,
) -> Result<Json<OkBody>, ApiError> {
    vpn_of(&lab)?
        .remove_lab(&network)
        .await
        .map_err(ApiError::Runtime)?;
    Ok(Json(OkBody { ok: true }))
}

pub fn router(lab: Arc<Lab>) -> Router {
    let authed = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/artifacts", get(artifacts))
        .route("/v1/log", get(log))
        .route("/v1/instances", post(create).get(list))
        .route("/v1/instances/{id}", get(get_one).delete(remove))
        .route("/v1/instances/{id}/stop", post(remove))
        .route("/v1/instances/{id}/restart", post(restart))
        .route("/v1/instances/{id}/screen", get(screen))
        .route("/v1/instances/{id}/verify-log", get(verify_log))
        .route("/v1/ready", get(ready))
        .route("/v1/vpn/files", axum::routing::put(vpn_files))
        .route(
            "/v1/vpn/gateway",
            axum::routing::put(vpn_ensure).get(vpn_state),
        )
        .route("/v1/vpn/attach", post(vpn_attach))
        .route("/v1/vpn/detach", post(vpn_detach))
        .route("/v1/vpn/rules", post(vpn_rules))
        .route("/v1/vpn/kick", post(vpn_kick))
        .route("/v1/vpn/reachable", get(vpn_reachable))
        .route(
            "/v1/vpn/labs/{network}",
            axum::routing::put(vpn_lab_put).delete(vpn_lab_delete),
        )
        .route("/v1/reconcile", post(reconcile))
        .route("/v1/cleanup", post(cleanup))
        .layer(middleware::from_fn_with_state(Arc::clone(&lab), auth));
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(authed)
        .with_state(lab)
}

/// Serves the API on `listener`, over TLS when an acceptor is given, until `shutdown` resolves.
pub async fn serve(
    listener: tokio::net::TcpListener,
    app: Router,
    tls: Option<tokio_rustls::TlsAcceptor>,
    shutdown: impl std::future::Future<Output = ()>,
) {
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use hyper_util::server::conn::auto::Builder;
    use hyper_util::service::TowerToHyperService;
    tokio::pin!(shutdown);
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::warn!("accept: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            },
            () = &mut shutdown => return,
        };
        let service =
            TowerToHyperService::new(app.clone().layer(axum::Extension(ConnectInfo(peer))));
        let tls = tls.clone();
        tokio::spawn(async move {
            let builder = Builder::new(TokioExecutor::new());
            let result = match tls {
                Some(acceptor) => {
                    match tokio::time::timeout(Duration::from_secs(10), acceptor.accept(stream))
                        .await
                    {
                        Ok(Ok(secured)) => {
                            builder
                                .serve_connection_with_upgrades(TokioIo::new(secured), service)
                                .await
                        }
                        // A failed or stalled handshake is a client that is not ours: drop it quietly.
                        _ => return,
                    }
                }
                None => {
                    builder
                        .serve_connection_with_upgrades(TokioIo::new(stream), service)
                        .await
                }
            };
            if let Err(e) = result {
                tracing::debug!("connection from {peer}: {e}");
            }
        });
    }
}

/// The container runtime of a lab host that only runs virtual machines.
pub struct Absent;

#[async_trait::async_trait]
impl InstanceRuntime for Absent {
    async fn start(&self, _: &StartRequest) -> Result<kymo_instances::Started, RuntimeError> {
        Err(RuntimeError::Unavailable(
            "this lab host does not run containers".into(),
        ))
    }
    async fn stop(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn status(&self, _: &str) -> Result<RuntimeStatus, RuntimeError> {
        Ok(RuntimeStatus::Missing)
    }
    async fn list_managed(&self) -> Result<Vec<kymo_instances::Managed>, RuntimeError> {
        Ok(Vec::new())
    }
    async fn ensure_lab(&self, _: &str, _: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Unavailable(
            "this lab host does not run containers".into(),
        ))
    }
    async fn remove_lab(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn reachable(&self, _: u16) -> bool {
        false
    }
}
