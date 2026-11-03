use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::error::ApiError;

pub const MIN_LENGTH: usize = 10;
pub const MAX_LENGTH: usize = 128;

/// Rejects passwords that are too short, or long enough to make hashing a denial of service.
pub fn check_strength(password: &str) -> Result<(), ApiError> {
    let len = password.chars().count();
    if !(MIN_LENGTH..=MAX_LENGTH).contains(&len) {
        return Err(ApiError::BadRequest(format!(
            "the password must be {MIN_LENGTH} to {MAX_LENGTH} characters"
        )));
    }
    Ok(())
}

/// Argon2 is deliberately slow, so it runs off the async workers.
pub async fn hash(password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::encode_b64(&rand::random::<[u8; 16]>()).ok()?;
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .ok()
            .map(|h| h.to_string())
    })
    .await
    .ok()
    .flatten()
    .ok_or(ApiError::Internal)
}

pub async fn verify(hash: String, candidate: String) -> bool {
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
