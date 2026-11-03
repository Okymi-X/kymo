//! Lab hosts as the platform uses them: a database row becomes a `RemoteRuntime`, cached so the
//! connection pool is reused, and rebuilt when the row changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use kymo_instances::{
    InstanceRuntime, Managed, RuntimeError, RuntimeStatus, StartRequest, Started,
};
use kymo_lab::RemoteRuntime;

use crate::error::ApiError;
use crate::lab_store::{self, LabHostRow};
use crate::state::{AppState, InstanceSetup};

#[derive(Default)]
pub struct LabRegistry {
    clients: Mutex<HashMap<i64, (String, Arc<RemoteRuntime>)>>,
}

/// A runtime together with the address players are told to use for it.
pub struct Target {
    pub runtime: Arc<dyn InstanceRuntime>,
    pub host: String,
}

/// The runtime of a platform with no containers of its own: every lab host does the work.
pub struct NoLocalRuntime;

fn none() -> RuntimeError {
    RuntimeError::Unavailable(
        "this server runs no containers itself: pick a lab host for the event".into(),
    )
}

#[async_trait]
impl InstanceRuntime for NoLocalRuntime {
    async fn start(&self, _: &StartRequest) -> Result<Started, RuntimeError> {
        Err(none())
    }
    async fn stop(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn status(&self, _: &str) -> Result<RuntimeStatus, RuntimeError> {
        Err(none())
    }
    async fn list_managed(&self) -> Result<Vec<Managed>, RuntimeError> {
        Ok(Vec::new())
    }
    async fn ensure_lab(&self, _: &str, _: &str) -> Result<(), RuntimeError> {
        Err(none())
    }
    async fn remove_lab(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn reachable(&self, _: u16) -> bool {
        false
    }
    fn can_start(&self) -> bool {
        false
    }
}

fn key(row: &LabHostRow) -> String {
    format!(
        "{}|{}|{}|{}",
        row.api_url,
        row.token,
        row.fingerprint.as_deref().unwrap_or(""),
        row.public_host
    )
}

impl LabRegistry {
    /// The client for a lab host row, reused while the row is unchanged.
    pub fn client(&self, row: &LabHostRow) -> Result<Arc<RemoteRuntime>, ApiError> {
        let key = key(row);
        let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((seen, client)) = clients.get(&row.id)
            && *seen == key
        {
            return Ok(Arc::clone(client));
        }
        let client = Arc::new(
            RemoteRuntime::new(
                &row.api_url,
                &row.token,
                row.fingerprint.as_deref(),
                &row.public_host,
            )
            .map_err(|e| ApiError::Unavailable(format!("lab host {}: {e}", row.name)))?,
        );
        clients.insert(row.id, (key, Arc::clone(&client)));
        Ok(client)
    }

    pub fn forget(&self, id: i64) {
        self.clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
}

/// Where a run lives: the lab host it was started on, or this machine's own runtime.
pub async fn target(
    state: &AppState,
    setup: &InstanceSetup,
    lab_host_id: Option<i64>,
) -> Result<Target, ApiError> {
    match lab_host_id {
        None => Ok(Target {
            runtime: Arc::clone(&setup.runtime),
            host: setup.public_host.clone(),
        }),
        Some(id) => {
            let row = lab_store::get(&state.pool, id).await?;
            let client = state.labs().client(&row)?;
            Ok(Target {
                host: row.public_host,
                runtime: client,
            })
        }
    }
}

/// A lab host that may take new instances: it exists and is switched on.
pub async fn usable(state: &AppState, id: i64) -> Result<LabHostRow, ApiError> {
    let row = lab_store::get(&state.pool, id).await?;
    if !row.enabled {
        return Err(ApiError::Unavailable(format!(
            "the lab host {} is switched off",
            row.name
        )));
    }
    Ok(row)
}

/// Asks every enabled lab host how it is and remembers the answer, so the admin pages show it
/// without calling out themselves. Called by the reaper.
pub async fn refresh_health(state: &AppState) {
    let Ok(rows) = lab_store::enabled(&state.pool).await else {
        return;
    };
    for row in rows {
        let Ok(client) = state.labs().client(&row) else {
            continue;
        };
        let error = client.health().await.err().map(|e| e.to_string());
        let _ = lab_store::record_check(&state.pool, row.id, state.now(), error.as_deref()).await;
    }
}
