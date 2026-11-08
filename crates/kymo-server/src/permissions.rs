//! Who may do what, as the organizer has configured it.
//!
//! The rules themselves (the defaults, which roles are locked) live in `kymo_core`. This module
//! keeps the configured matrix in memory, loads it lazily from the `setting` table and stores
//! only the differences from the defaults, so a permission added in a later version follows its
//! default until an organizer decides otherwise.

use std::sync::RwLock;

use kymo_core::{Permission, PermissionMatrix, Role};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::OnceCell;

use crate::error::ApiError;

const KEY: &str = "role_permissions";

/// The in memory copy. Never held across an await.
#[derive(Default)]
pub struct PermissionCache {
    matrix: RwLock<PermissionMatrix>,
    loaded: OnceCell<()>,
}

impl PermissionCache {
    fn snapshot(&self) -> PermissionMatrix {
        self.matrix
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn replace(&self, matrix: PermissionMatrix) {
        *self.matrix.write().unwrap_or_else(|e| e.into_inner()) = matrix;
        // A change made before the first request must not be overwritten by the lazy load.
        let _ = self.loaded.set(());
    }

    /// The current matrix, loading it from the database the first time.
    pub async fn matrix(&self, pool: &SqlitePool) -> Result<PermissionMatrix, ApiError> {
        self.loaded
            .get_or_try_init(|| async {
                let matrix = load(pool).await?;
                *self.matrix.write().unwrap_or_else(|e| e.into_inner()) = matrix;
                Ok::<(), ApiError>(())
            })
            .await?;
        Ok(self.snapshot())
    }

    /// Stores the matrix and makes it current. The caller holds the write lock when it read the
    /// old matrix first.
    pub async fn set(&self, pool: &SqlitePool, matrix: PermissionMatrix) -> Result<(), ApiError> {
        save(pool, &matrix).await?;
        self.replace(matrix);
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Stored {
    role: String,
    permission: String,
    allowed: bool,
}

/// Reads the stored differences. Anything that no longer parses (a role or permission from a
/// different version, a damaged row) is ignored rather than failing every request.
pub async fn load(pool: &SqlitePool) -> Result<PermissionMatrix, ApiError> {
    let stored: Option<String> = sqlx::query_scalar("SELECT value FROM setting WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await?;
    let Some(json) = stored else {
        return Ok(PermissionMatrix::default());
    };
    let rows: Vec<Stored> = match serde_json::from_str(&json) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("stored role permissions are unreadable, using the defaults: {e}");
            return Ok(PermissionMatrix::default());
        }
    };
    Ok(PermissionMatrix::from_overrides(
        rows.into_iter().filter_map(|row| {
            let role = row.role.parse::<Role>().ok()?;
            let permission = row.permission.parse::<Permission>().ok()?;
            Some((role, permission, row.allowed))
        }),
    ))
}

async fn save(pool: &SqlitePool, matrix: &PermissionMatrix) -> Result<(), ApiError> {
    let rows: Vec<Stored> = matrix
        .overrides()
        .into_iter()
        .map(|(role, permission, allowed)| Stored {
            role: role.as_str().to_owned(),
            permission: permission.as_str().to_owned(),
            allowed,
        })
        .collect();
    let json = serde_json::to_string(&rows).map_err(|_| ApiError::Internal)?;
    sqlx::query("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(KEY)
        .bind(json)
        .execute(pool)
        .await?;
    Ok(())
}
