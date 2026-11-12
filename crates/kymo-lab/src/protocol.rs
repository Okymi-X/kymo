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
//!
//! With the lab host's own VPN gateway enabled (`KYMO_LAB_VPN_HOST`), players reach instances
//! through a tunnel to the lab host instead of through published ports:
//!
//! | `PUT  /v1/vpn/files`               | Replace the gateway's certificates, config, rules       |
//! | `PUT  /v1/vpn/gateway`             | Make sure the gateway container is running              |
//! | `GET  /v1/vpn/gateway`             | Whether it runs and which lab networks it is on         |
//! | `POST /v1/vpn/attach` / `detach`   | Join or leave a lab network                             |
//! | `POST /v1/vpn/rules`               | Apply the firewall script                               |
//! | `POST /v1/vpn/kick`                | End a player's session                                  |
//! | `GET  /v1/vpn/connected`           | Which players are connected to the gateway right now    |
//! | `GET  /v1/vpn/reachable?ip=&port=` | Whether a service answers, asked from inside the gateway |
//! | `PUT  /v1/vpn/labs/{network}`      | Create a private lab network with the given subnet      |
//! | `DELETE /v1/vpn/labs/{network}`    | Remove it                                               |

use std::collections::BTreeMap;

use kymo_instances::{Endpoint, ExtraPort, InstanceSpec, RunFlag};
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
    /// Put the instance on a private lab network, with nothing published on the host.
    #[serde(default)]
    pub lab: Option<LabPlacement>,
    /// Flags the platform generated for this run. The lab host puts each where its manifest
    /// says, or keeps it for the Verifier. Absent for an ordinary instance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<RunFlag>,
}

/// A private lab network (made with `PUT /v1/vpn/labs/{network}`) and the instance's place in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabPlacement {
    pub network: String,
    /// The address a container takes on the network.
    pub ip: String,
    /// The network's own address on the lab host (its `.1`), which names the bridge a virtual
    /// machine's tap is plugged into.
    pub bind: String,
    /// The address the platform allocated for the run's Verifier container, taken from the same
    /// network like the instance's own. Present for a Responder, absent for every other
    /// instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_ip: Option<String>,
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
    /// Players are given just the machine's address, with every port open on it, instead of the
    /// address of one service.
    #[serde(default)]
    pub machine: bool,
    pub created_at: i64,
    pub expires_at: i64,
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
    /// The instance's address on its lab network.
    #[serde(default)]
    pub ip: Option<String>,
    /// Where the instance, and its Verifier if it has one, can be reached. Flag values are
    /// never part of an instance record.
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
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
    /// The lab host's own VPN gateway, when it has one.
    #[serde(default)]
    pub vpn: Option<VpnInfo>,
    /// True when the lab host refuses to publish ports: every instance must sit on a lab network.
    #[serde(default)]
    pub vpn_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnInfo {
    /// The name or address players connect their VPN client to.
    pub host: String,
    pub port: u16,
    /// The address range lab networks may use.
    pub pool: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnFile {
    pub content: String,
    /// Written readable by the owner only.
    #[serde(default)]
    pub secret: bool,
}

/// Every file the gateway reads, by path under its directory. Files not listed under `ccd/`
/// are removed, so a player who is no longer listed cannot connect.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VpnFiles {
    pub files: BTreeMap<String, VpnFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkBody {
    pub network: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabNetBody {
    pub subnet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KickBody {
    pub common_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OkBody {
    pub ok: bool,
}

/// The players connected to the gateway, by common name (`u<id>`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConnectedState {
    pub connected: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GatewayState {
    pub running: bool,
    pub networks: Vec<String>,
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
