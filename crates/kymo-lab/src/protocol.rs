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
//! | `GET  /v1/instances/{id}/screen`   | A virtual machine's console as a PNG                    |
//! | `GET  /v1/ready?port=N`            | Whether the service behind a published port answers     |
//! | `POST /v1/instances/{id}/restart`  | Remove it and start it again under the same name        |
//! | `POST /v1/instances/{id}/stop`     | Remove it (same as DELETE, instances are disposable)    |
//! | `DELETE /v1/instances/{id}`        | Remove it. Removing a missing instance succeeds         |
//! | `POST /v1/reconcile`               | Remove every instance the platform no longer wants      |
//! | `POST /v1/cleanup`                 | Remove the expired ones now                             |
//! | `GET  /v1/log`                     | The lab host's recent actions                           |

use std::collections::BTreeMap;

use kymo_instances::{ExtraPort, InstanceSpec};
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
    /// Further published services, for a machine that offers several.
    #[serde(default)]
    pub ports: Vec<ExtraPort>,
    /// `docker`, `podman` or `vm`.
    #[serde(default)]
    pub runtime: String,
    /// How long the platform should wait for the service to answer, when it is more than the
    /// usual minute (a virtual machine boots slowly).
    #[serde(default)]
    pub ready_secs: Option<u32>,
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
    /// What this lab host can run: `docker`, `vm`.
    #[serde(default)]
    pub runtimes: Vec<String>,
    /// How virtual machines are accelerated: `kvm`, or `tcg` (plain emulation, slow).
    #[serde(default)]
    pub accel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub artifact: String,
    pub runtime: String,
    pub port: u16,
    pub max_ttl_secs: i64,
    pub env_allow: Vec<String>,
    /// Every published service, primary first.
    #[serde(default)]
    pub ports: Vec<PortDecl>,
    /// False while a virtual machine image is still being unpacked or could not be.
    #[serde(default = "yes")]
    pub ready: bool,
    /// What is wrong or in progress, in words.
    #[serde(default)]
    pub note: Option<String>,
}

fn yes() -> bool {
    true
}

/// A named service port of a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDecl {
    #[serde(default = "main_name")]
    pub name: String,
    pub port: u16,
}

fn main_name() -> String {
    "main".into()
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

/// Whether the service behind a published port answers. For a virtual machine the lab host knows
/// better than a connection can tell: QEMU accepts the host side of a forwarded port while the
/// guest is still booting. `vouched` says the lab host checked the guest itself, so the platform
/// only needs to confirm the port is reachable from outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadyInfo {
    pub ready: bool,
    pub vouched: bool,
}
