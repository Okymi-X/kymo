use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

const FAILURE_WINDOW: Duration = Duration::from_secs(60);
const MAX_FAILURES: usize = 5;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub events: broadcast::Sender<String>,
    /// Serializes read-modify-write cycles on a bracket.
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    admin_hash: Arc<String>,
    failures: Arc<Mutex<VecDeque<Instant>>>,
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

    /// True while too many recent sign in attempts have failed. One shared budget keeps
    /// the check independent of proxy headers, at the cost of a brief organizer lockout.
    pub fn login_blocked(&self) -> bool {
        let mut failures = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        while failures
            .front()
            .is_some_and(|at| at.elapsed() > FAILURE_WINDOW)
        {
            failures.pop_front();
        }
        failures.len() >= MAX_FAILURES
    }

    pub fn record_login(&self, success: bool) {
        let mut failures = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        if success {
            failures.clear();
        } else {
            failures.push_back(Instant::now());
        }
    }

    pub fn notify(&self, slug: &str) {
        let _ = self.events.send(slug.to_owned());
    }
}
