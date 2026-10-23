use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub events: broadcast::Sender<String>,
    /// Serializes read-modify-write cycles on a bracket.
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    admin_hash: Arc<String>,
    sessions: Arc<Mutex<HashMap<String, Instant>>>,
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
            sessions: Arc::default(),
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

    pub fn open_session(&self) -> String {
        let token: String = rand::random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.retain(|_, born| born.elapsed() < SESSION_TTL);
        sessions.insert(token.clone(), Instant::now());
        token
    }

    pub fn session_is_valid(&self, token: &str) -> bool {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions
            .get(token)
            .is_some_and(|born| born.elapsed() < SESSION_TTL)
    }

    pub fn notify(&self, slug: &str) {
        let _ = self.events.send(slug.to_owned());
    }
}
