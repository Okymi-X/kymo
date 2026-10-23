use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

const FAILURE_WINDOW: Duration = Duration::from_secs(60);
const MAX_FAILURES: usize = 5;
const MAX_TRACKED_CLIENTS: usize = 10_000;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub events: broadcast::Sender<String>,
    /// Serializes read-modify-write cycles on a bracket.
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    admin_hash: Arc<String>,
    failures: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
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
            failures: Arc::default(),
        })
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

    pub fn notify(&self, slug: &str) {
        let _ = self.events.send(slug.to_owned());
    }
}
