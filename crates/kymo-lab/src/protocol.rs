//! The lab API, as the platform and the lab host both see it. Everything is JSON over HTTPS,
//! under `/v1`, with a bearer token.
//!
//! | Method and path                    | What it does                                            |
//! |------------------------------------|---------------------------------------------------------|
//! | `GET  /healthz`                    | Liveness, no token (for proxies and monitors)           |
//! | `GET  /v1/health`                  | Version, runtime, load, mode                            |
//! | `GET  /v1/artifacts`               | The images this lab host will run (its manifests)       |
//! | `POST /v1/instances`               | Create and start an instance under the given name       |
//! | `GET  /v1/instances`               | Every instance the lab runs                             |
//! | `GET  /v1/instances/{id}`          | One instance, by name, id or platform instance id       |
//! | `POST /v1/instances/{id}/restart`  | Remove it and start it again under the same name        |
//! | `POST /v1/instances/{id}/stop`     | Remove it (same as DELETE, instances are disposable)    |
//! | `DELETE /v1/instances/{id}`        | Remove it. Removing a missing instance succeeds         |
//! | `POST /v1/reconcile`               | Remove every instance the platform no longer wants      |
//! | `POST /v1/cleanup`                 | Remove the expired ones now                             |
//! | `GET  /v1/log`                     | The lab host's recent actions                           |

use std::collections::BTreeMap;

use kymo_instances::InstanceSpec;
use serde::{Deserialize, Serialize};

pub const API_VERSION: &str = "v1";

/// Create and start one instance. The platform builds `name` and records it before it asks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequest {
    /// Container name: lowercase letters, digits and dashes, starting with the lab's prefix.
    pub name: String,
    /// The platform's id for the run, kept so the platform can find what it started.
    pub instance_id: String,
    /// What to run. In manifest mode `spec.image` names an artifact the lab host lists, and the
    /// manifest decides the port and limits; the platform can only ask for less.
    pub spec: InstanceSpec,
    /// Event, challenge and owner, for the lab host's log and labels.
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInfo {
    /// The runtime's handle (container id).
    pub id: String,
    pub name: String,
    pub instance_id: String,
    /// `running`, `exited` or `missing`.
    pub status: String,
    /// The published port on the lab host.
    pub host_port: u16,
    pub created_at: i64,
    pub expires_at: i64,
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthInfo {
    /// The lab host's own name.
    pub name: String,
    pub version: String,
    pub api: String,
    /// `docker` or `podman`, whatever the lab host runs instances with.
    pub runtime: String,
    /// `manifest` (only listed artifacts run) or `open`.
    pub mode: String,
    pub running: usize,
    pub max_instances: usize,
    pub artifacts: usize,
    pub uptime_secs: i64,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub artifact: String,
    pub runtime: String,
    pub port: u16,
    pub max_ttl_secs: i64,
    pub env_allow: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileRequest {
    /// The platform instance ids that should be running. Everything else is removed.
    pub keep: Vec<String>,
    /// Instances younger than this are left alone, so one still being created is never taken
    /// for an orphan. Default 60.
    #[serde(default)]
    pub grace_secs: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Removed {
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    pub at: i64,
    pub action: String,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}
