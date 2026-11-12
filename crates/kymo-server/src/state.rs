use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::clock::{self, Clock};
use crate::error::ApiError;
use crate::labs::LabRegistry;
use crate::monitor::Monitor;
use crate::ops::{BackupConfig, Ops};
use crate::permissions::PermissionCache;
use crate::vpn::VpnSetup;
use kymo_core::{Permission, PermissionMatrix, Role};
use kymo_instances::{InstanceRuntime, Policy, ResourceCaps};
use kymo_jeopardy::{RateDenied, RateRule, RateState};

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

const FAILURE_WINDOW: Duration = Duration::from_secs(60);
const MAX_FAILURES: usize = 5;
const MAX_TRACKED_CLIENTS: usize = 10_000;
const MAX_TRACKED_PARTICIPANTS: usize = 100_000;

type RateKey = (i64, i64);

/// The container platform and the rules around it. Absent when instances are not enabled.
#[derive(Clone)]
pub struct InstanceSetup {
    pub runtime: Arc<dyn InstanceRuntime>,
    pub policy: Policy,
    pub caps: ResourceCaps,
    /// Host name players are told to connect to.
    pub public_host: String,
    /// Seconds an expired instance may linger before the reaper stops it.
    pub reap_grace_secs: i64,
}

/// Answers kept for anonymous visitors of public views.
pub type PublicCache = Mutex<HashMap<String, (i64, bytes::Bytes, String)>>;

/// Per lab host: when it was asked and who was connected.
pub type ConnectedCache = Mutex<HashMap<i64, (i64, Vec<String>)>>;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub events: broadcast::Sender<String>,
    /// Serializes read-modify-write cycles on a bracket.
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    admin_hash: Arc<String>,
    clock: Clock,
    failures: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
    /// Flag submission limiter per (event, scoring participant). Lost on restart, which only
    /// ever makes it more lenient for a moment.
    rates: Arc<Mutex<HashMap<RateKey, RateState>>>,
    instances: Option<InstanceSetup>,
    monitor: Arc<Monitor>,
    trust_proxy: bool,
    ops: Arc<Ops>,
    backups: Option<BackupConfig>,
    vpn: Option<VpnSetup>,
    labs: Arc<LabRegistry>,
    permissions: Arc<PermissionCache>,
    /// Who was connected to each gateway when it was last asked (host, when, common names), so
    /// many browsers polling cost a lab host one question every few seconds.
    connected: Arc<ConnectedCache>,
    /// The last answer to an anonymous visitor of a public view, by request: when, body, type.
    public_cache: Arc<PublicCache>,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        admin_password: &str,
    ) -> Result<Self, argon2::password_hash::Error> {
        let salt = SaltString::encode_b64(&rand::random::<[u8; 16]>())?;
        let hash = Argon2::default().hash_password(admin_password.as_bytes(), &salt)?;
        Ok(Self {
            pool,
            events: broadcast::channel(64).0,
            write_lock: Arc::default(),
            admin_hash: Arc::new(hash.to_string()),
            clock: clock::system(),
            failures: Arc::default(),
            rates: Arc::default(),
            instances: None,
            monitor: Arc::default(),
            trust_proxy: false,
            ops: Arc::default(),
            backups: None,
            vpn: None,
            labs: Arc::default(),
            permissions: Arc::default(),
            connected: Arc::default(),
            public_cache: Arc::default(),
        })
    }

    /// Replaces the time source, so tests can fix or move the server's notion of now.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_instances(mut self, setup: InstanceSetup) -> Self {
        self.instances = Some(setup);
        self
    }

    /// Believe the last `X-Forwarded-For` entry. Only enable behind a reverse proxy that
    /// sets it, otherwise any client could pick its own address.
    pub fn with_trusted_proxy(mut self, trust: bool) -> Self {
        self.trust_proxy = trust;
        self
    }

    pub fn trusts_proxy(&self) -> bool {
        self.trust_proxy
    }

    pub fn with_vpn(mut self, setup: VpnSetup) -> Self {
        self.vpn = Some(setup);
        self
    }

    pub fn public_cache(&self) -> &PublicCache {
        &self.public_cache
    }

    pub fn connected_cache(&self) -> &ConnectedCache {
        &self.connected
    }

    pub fn vpn_setup(&self) -> Option<&VpnSetup> {
        self.vpn.as_ref()
    }

    pub fn with_backups(mut self, config: BackupConfig) -> Self {
        self.backups = Some(config);
        self
    }

    pub fn backup_config(&self) -> Option<&BackupConfig> {
        self.backups.as_ref()
    }

    pub fn ops(&self) -> &Ops {
        &self.ops
    }

    pub fn monitor(&self) -> &Monitor {
        &self.monitor
    }

    /// Connections to lab hosts, built from their database rows.
    pub fn labs(&self) -> &LabRegistry {
        &self.labs
    }

    pub fn instance_setup(&self) -> Option<&InstanceSetup> {
        self.instances.as_ref()
    }

    /// Whether the role holds the permission under the organizer's configuration. Every
    /// permission decision goes through here, so no route can bypass the configured matrix.
    pub async fn can(&self, role: Role, permission: Permission) -> Result<bool, ApiError> {
        Ok(self.permission_matrix().await?.can(role, permission))
    }

    pub async fn permission_matrix(&self) -> Result<PermissionMatrix, ApiError> {
        self.permissions.matrix(&self.pool).await
    }

    pub async fn set_permission_matrix(&self, matrix: PermissionMatrix) -> Result<(), ApiError> {
        self.permissions.set(&self.pool, matrix).await
    }

    /// Current unix time in seconds from the configured clock.
    pub fn now(&self) -> i64 {
        (self.clock)()
    }

    /// A valid hash nobody knows the password for, used to spend the same time on unknown
    /// usernames as on wrong passwords.
    pub fn decoy_hash(&self) -> String {
        self.admin_hash.as_str().to_owned()
    }

    /// Argon2 is deliberately slow, so it runs off the async workers.
    pub async fn verify_password(&self, candidate: String) -> bool {
        let hash = Arc::clone(&self.admin_hash);
        tokio::task::spawn_blocking(move || {
            PasswordHash::new(&hash)
                .map(|parsed| {
                    Argon2::default()
                        .verify_password(candidate.as_bytes(), &parsed)
                        .is_ok()
                })
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }

    /// True while this client has too many recent failed sign ins. Each client address has
    /// its own budget, so one caller cannot lock the organizer out. Behind a reverse proxy
    /// every client shares the proxy address, which makes the budget global again.
    pub fn login_blocked(&self, client: IpAddr) -> bool {
        let mut all = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        let Some(failures) = all.get_mut(&client) else {
            return false;
        };
        while failures
            .front()
            .is_some_and(|at| at.elapsed() > FAILURE_WINDOW)
        {
            failures.pop_front();
        }
        if failures.is_empty() {
            all.remove(&client);
            return false;
        }
        failures.len() >= MAX_FAILURES
    }

    pub fn record_login(&self, client: IpAddr, success: bool) {
        let mut all = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        if success {
            all.remove(&client);
            return;
        }
        if all.len() >= MAX_TRACKED_CLIENTS && !all.contains_key(&client) {
            all.clear();
        }
        all.entry(client).or_default().push_back(Instant::now());
    }

    /// Records a counted flag attempt for the participant or says how long to wait.
    pub fn rate_check(
        &self,
        event_id: i64,
        participant_id: i64,
        rule: &RateRule,
    ) -> Result<(), RateDenied> {
        let now = self.now();
        let mut all = self.rates.lock().unwrap_or_else(|e| e.into_inner());
        let key = (event_id, participant_id);
        if all.len() >= MAX_TRACKED_PARTICIPANTS && !all.contains_key(&key) {
            all.clear();
        }
        all.entry(key).or_default().check_and_record(rule, now)
    }

    pub fn notify(&self, slug: &str) {
        let _ = self.events.send(slug.to_owned());
    }
}
