//! The container platform seen from the platform: start, stop, look, list. Docker is one
//! implementation; the in memory fake serves tests and a platform without containers.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;
use crate::spec::InstanceSpec;

/// Where an instance is reachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// A private network of its own, with the service port published on the host.
    Published,
    /// A named lab network (made with `ensure_lab`) at a fixed address. Nothing is published
    /// on the host; the service is reachable only from inside the lab. `gateway` is the
    /// network's own address on the host, used where a machine cannot join the network itself
    /// (a virtual machine forwards its ports there).
    Lab {
        network: String,
        ip: String,
        gateway: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct StartRequest {
    /// Platform side id, stored as a label so orphans can be found after a crash.
    pub instance_id: String,
    pub spec: InstanceSpec,
    pub placement: Placement,
    /// The container's name, built by the platform before it asks (`kymo-c3-u7-a1b2c3`). Safe
    /// characters only. When absent the runtime derives one from `instance_id`.
    pub name: Option<String>,
    /// Who and what this instance is for (event, challenge, owner), kept as labels so a lab
    /// host can tell what it is running and log it.
    pub meta: Vec<(String, String)>,
}

impl StartRequest {
    /// An instance with its port published on the host.
    pub fn new(instance_id: impl Into<String>, spec: InstanceSpec) -> Self {
        Self {
            instance_id: instance_id.into(),
            spec,
            placement: Placement::Published,
            name: None,
            meta: Vec::new(),
        }
    }
}

/// How a flag reaches the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagDelivery {
    /// The lab host writes it into the instance (an environment variable, or a file over SSH).
    Inject,
    /// The lab host keeps it and only the Verifier container can release it.
    Hold,
}

/// A flag the platform generated for one run. The value is a secret: it never appears in
/// `Debug` output, so it cannot end up in a log line by accident.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFlag {
    pub name: String,
    pub value: String,
    pub deliver: FlagDelivery,
}

impl std::fmt::Debug for RunFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunFlag")
            .field("name", &self.name)
            .field("value", &"<hidden>")
            .field("deliver", &self.deliver)
            .finish()
    }
}

/// Something an instance is reachable at, as the lab host reports it: the machine, a Verifier,
/// or a listed service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// `machine`, `verifier` or `service`.
    pub role: String,
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    /// `http` for a web page, none for a plain address.
    #[serde(default)]
    pub scheme: Option<String>,
}

/// What a start may carry besides the request: generated flags, and the address of the run's
/// Verifier. Kept apart from `StartRequest` so runtimes that know nothing about them are
/// unaffected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartExtras {
    pub flags: Vec<RunFlag>,
    pub verifier_ip: Option<String>,
}

impl StartExtras {
    pub fn is_empty(&self) -> bool {
        self.flags.is_empty() && self.verifier_ip.is_none()
    }
}

/// A Verifier container to start next to an instance, on the same lab network.
#[derive(Clone, PartialEq, Eq)]
pub struct VerifierSpec {
    /// The container's name, built from the instance's.
    pub name: String,
    /// The instance it belongs to, kept as a label so cleanup can tell what is orphaned.
    pub instance_id: String,
    pub network: String,
    pub ip: String,
    pub image: String,
    pub port: u16,
    pub memory_mb: u32,
    pub cpu_millis: u32,
    /// The host directory with `verifier.json` and the scripts, mounted read-only.
    pub definition_dir: String,
    /// A JSON document describing the run (target address, held flag, admin token). A secret:
    /// it stays out of `Debug` output.
    pub run_json: String,
}

impl std::fmt::Debug for VerifierSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifierSpec")
            .field("name", &self.name)
            .field("instance_id", &self.instance_id)
            .field("network", &self.network)
            .field("ip", &self.ip)
            .field("image", &self.image)
            .field("port", &self.port)
            .field("run_json", &"<hidden>")
            .finish()
    }
}

/// A started instance together with the endpoints the lab host reported for it.
#[derive(Debug, Clone)]
pub struct StartOutcome {
    pub started: Started,
    pub endpoints: Vec<Endpoint>,
}

/// A further published service of an instance, for machines that offer several (a web server and
/// SSH, say). `host` is where it is reachable, `guest` the port inside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraPort {
    pub name: String,
    pub guest: u16,
    pub host: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub handle: String,
    /// The published host port, or the service port for a lab placement.
    pub host_port: u16,
    /// The lab address, for a lab placement.
    pub ip: Option<String>,
    /// More published services, besides the one in `host_port`.
    pub extra_ports: Vec<ExtraPort>,
    /// How long the platform should wait for the service to answer before it counts the start as
    /// failed. A virtual machine takes far longer to boot than a container; none means the default.
    pub ready_secs: Option<u32>,
    /// Players are given just the address, with every port open on it (a machine to scan), rather
    /// than the address of one service.
    pub machine: bool,
}

impl Started {
    pub fn new(handle: impl Into<String>, host_port: u16, ip: Option<String>) -> Self {
        Self {
            handle: handle.into(),
            host_port,
            ip,
            extra_ports: Vec::new(),
            ready_secs: None,
            machine: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStatus {
    Running,
    Exited,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Managed {
    pub handle: String,
    pub instance_id: String,
}

#[async_trait]
pub trait InstanceRuntime: Send + Sync {
    async fn start(&self, request: &StartRequest) -> Result<Started, RuntimeError>;
    /// Starts with generated flags and a Verifier address. Only a lab host can do this, so the
    /// default refuses rather than silently starting an instance without its flags.
    async fn start_with(
        &self,
        request: &StartRequest,
        extras: &StartExtras,
    ) -> Result<StartOutcome, RuntimeError> {
        if !extras.is_empty() {
            return Err(RuntimeError::Failed(
                "this runtime cannot hand out flags or a Verifier, use a lab host".into(),
            ));
        }
        Ok(StartOutcome {
            started: self.start(request).await?,
            endpoints: Vec::new(),
        })
    }
    /// Removes the container and anything created for it. Stopping a missing instance is Ok.
    async fn stop(&self, handle: &str) -> Result<(), RuntimeError>;
    async fn status(&self, handle: &str) -> Result<RuntimeStatus, RuntimeError>;
    /// Every container this platform created, running or not.
    async fn list_managed(&self) -> Result<Vec<Managed>, RuntimeError>;
    /// Makes an isolated network with no route out and no route to other labs. `subnet` is
    /// CIDR text. Making a lab that already exists is fine.
    async fn ensure_lab(&self, network: &str, subnet: &str) -> Result<(), RuntimeError>;
    /// Removes a lab network. Every container must have left it. A missing lab is fine.
    async fn remove_lab(&self, network: &str) -> Result<(), RuntimeError>;
    /// Whether something accepts TCP connections on the published host port. Only meaningful
    /// for a published placement; a lab is probed through the VPN gateway instead.
    async fn reachable(&self, host_port: u16) -> bool;
    /// Starts a Verifier container on a lab network and returns its handle. Only a runtime that
    /// can place containers on lab networks can.
    async fn start_verifier(&self, _spec: &VerifierSpec) -> Result<String, RuntimeError> {
        Err(RuntimeError::Unavailable(
            "this runtime cannot start a Verifier".into(),
        ))
    }
    /// Every Verifier container this runtime started, running or not.
    async fn list_verifiers(&self) -> Result<Vec<Managed>, RuntimeError> {
        Ok(Vec::new())
    }
    /// Whether a started instance has finished its own setup (flags written, Verifier up) and
    /// may be handed to a player. Only a lab host has such a step; everything else is ready as
    /// soon as it runs.
    async fn ready(&self, _handle: &str) -> bool {
        true
    }
    /// Whether this runtime can start anything. A platform that leaves all containers to lab
    /// hosts says no, so an event with no lab host can be told why it cannot start instances.
    fn can_start(&self) -> bool {
        true
    }
}

#[derive(Default)]
struct FakeInner {
    next: u16,
    live: HashMap<String, (String, InstanceSpec)>,
    placements: HashMap<String, Placement>,
    labs: HashMap<String, String>,
    exited: std::collections::HashSet<String>,
    not_reachable: bool,
    fail_starts: bool,
    extra_ports: Vec<ExtraPort>,
    ready_secs: Option<u32>,
    verifiers: HashMap<String, VerifierSpec>,
}

/// In memory runtime for tests.
#[derive(Default)]
pub struct FakeRuntime {
    inner: Mutex<FakeInner>,
}

impl FakeRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every start report these extra published services and boot time, as a virtual
    /// machine with several services would.
    pub fn set_extras(&self, extra_ports: Vec<ExtraPort>, ready_secs: Option<u32>) {
        let mut inner = self.lock();
        inner.extra_ports = extra_ports;
        inner.ready_secs = ready_secs;
    }

    pub fn set_fail_starts(&self, fail: bool) {
        self.lock().fail_starts = fail;
    }

    pub fn running_count(&self) -> usize {
        self.lock().live.len()
    }

    /// Handles of every running container, oldest first.
    pub fn list_handles(&self) -> Vec<String> {
        let mut handles: Vec<String> = self.lock().live.keys().cloned().collect();
        handles.sort();
        handles
    }

    /// Makes `reachable` answer false, as a service that has not finished starting would.
    pub fn set_unreachable(&self, unreachable: bool) {
        self.lock().not_reachable = unreachable;
    }

    /// Simulates a container that exited on its own (it still exists, no longer running).
    pub fn set_exited(&self, handle: &str) {
        self.lock().exited.insert(handle.to_owned());
    }

    /// Lab networks that exist, as (network, subnet).
    pub fn labs(&self) -> Vec<(String, String)> {
        let mut labs: Vec<_> = self
            .lock()
            .labs
            .iter()
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        labs.sort();
        labs
    }

    /// Where a running handle was placed.
    pub fn placement_of(&self, handle: &str) -> Option<Placement> {
        self.lock().placements.get(handle).cloned()
    }

    /// The spec a handle was started with, for asserting limits and environment.
    pub fn spec_of(&self, handle: &str) -> Option<InstanceSpec> {
        self.lock().live.get(handle).map(|(_, spec)| spec.clone())
    }

    /// The Verifier containers that are running, by handle.
    pub fn verifiers(&self) -> Vec<(String, VerifierSpec)> {
        let mut list: Vec<_> = self
            .lock()
            .verifiers
            .iter()
            .map(|(h, v)| (h.clone(), v.clone()))
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        list
    }

    /// Simulates a container nobody recorded, as left behind by a crash.
    pub fn add_orphan(&self, instance_id: &str, spec: InstanceSpec) -> String {
        let mut inner = self.lock();
        inner.next += 1;
        let handle = format!("fake-{}", inner.next);
        inner
            .live
            .insert(handle.clone(), (instance_id.to_owned(), spec));
        handle
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl InstanceRuntime for FakeRuntime {
    async fn start(&self, request: &StartRequest) -> Result<Started, RuntimeError> {
        let mut inner = self.lock();
        if inner.fail_starts {
            return Err(RuntimeError::Failed("image not found".into()));
        }
        let ip = match &request.placement {
            Placement::Published => None,
            Placement::Lab { network, ip, .. } => {
                if !inner.labs.contains_key(network) {
                    return Err(RuntimeError::Failed(format!("no lab network {network}")));
                }
                let taken = inner.placements.iter().any(|(h, p)| {
                    inner.live.contains_key(h) && matches!(p, Placement::Lab { network: n, ip: i, .. } if n == network && i == ip)
                });
                if taken {
                    return Err(RuntimeError::Failed(format!("{ip} is already in use")));
                }
                Some(ip.clone())
            }
        };
        inner.next += 1;
        let handle = format!("fake-{}", inner.next);
        let host_port = if ip.is_some() {
            request.spec.port
        } else {
            30000 + inner.next
        };
        inner.live.insert(
            handle.clone(),
            (request.instance_id.clone(), request.spec.clone()),
        );
        inner
            .placements
            .insert(handle.clone(), request.placement.clone());
        let mut started = Started::new(handle, host_port, ip);
        started.extra_ports = inner.extra_ports.clone();
        started.ready_secs = inner.ready_secs;
        Ok(started)
    }

    async fn stop(&self, handle: &str) -> Result<(), RuntimeError> {
        let mut inner = self.lock();
        inner.live.remove(handle);
        inner.placements.remove(handle);
        inner.verifiers.remove(handle);
        Ok(())
    }

    async fn start_verifier(&self, spec: &VerifierSpec) -> Result<String, RuntimeError> {
        let mut inner = self.lock();
        if inner.fail_starts {
            return Err(RuntimeError::Failed("verifier image not found".into()));
        }
        if !inner.labs.contains_key(&spec.network) {
            return Err(RuntimeError::Failed(format!(
                "no lab network {}",
                spec.network
            )));
        }
        inner.next += 1;
        let handle = format!("fake-verifier-{}", inner.next);
        inner.verifiers.insert(handle.clone(), spec.clone());
        Ok(handle)
    }

    async fn list_verifiers(&self) -> Result<Vec<Managed>, RuntimeError> {
        Ok(self
            .lock()
            .verifiers
            .iter()
            .map(|(handle, v)| Managed {
                handle: handle.clone(),
                instance_id: v.instance_id.clone(),
            })
            .collect())
    }

    async fn status(&self, handle: &str) -> Result<RuntimeStatus, RuntimeError> {
        let inner = self.lock();
        Ok(if !inner.live.contains_key(handle) {
            RuntimeStatus::Missing
        } else if inner.exited.contains(handle) {
            RuntimeStatus::Exited
        } else {
            RuntimeStatus::Running
        })
    }

    async fn list_managed(&self) -> Result<Vec<Managed>, RuntimeError> {
        Ok(self
            .lock()
            .live
            .iter()
            .map(|(handle, (id, _))| Managed {
                handle: handle.clone(),
                instance_id: id.clone(),
            })
            .collect())
    }

    async fn ensure_lab(&self, network: &str, subnet: &str) -> Result<(), RuntimeError> {
        self.lock()
            .labs
            .insert(network.to_owned(), subnet.to_owned());
        Ok(())
    }

    async fn reachable(&self, _host_port: u16) -> bool {
        !self.lock().not_reachable
    }

    async fn remove_lab(&self, network: &str) -> Result<(), RuntimeError> {
        let mut inner = self.lock();
        let in_use = inner
            .placements
            .values()
            .any(|p| matches!(p, Placement::Lab { network: n, .. } if n == network));
        if in_use {
            return Err(RuntimeError::Failed(format!(
                "{network} still has containers"
            )));
        }
        inner.labs.remove(network);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> InstanceSpec {
        InstanceSpec {
            image: "img:1".into(),
            port: 80,
            cpu_millis: 100,
            memory_mb: 64,
            pids: 32,
            ttl_secs: 60,
            env: vec![],
            read_only: true,
        }
    }

    #[tokio::test]
    async fn start_stop_list_and_status_agree() {
        let rt = FakeRuntime::new();
        let req = StartRequest::new("7", spec());
        let started = rt.start(&req).await.unwrap();
        assert_eq!(
            rt.status(&started.handle).await.unwrap(),
            RuntimeStatus::Running
        );
        assert_eq!(rt.list_managed().await.unwrap().len(), 1);
        rt.stop(&started.handle).await.unwrap();
        assert_eq!(
            rt.status(&started.handle).await.unwrap(),
            RuntimeStatus::Missing
        );
        assert!(rt.stop(&started.handle).await.is_ok(), "stop is idempotent");
        assert_eq!(rt.running_count(), 0);
    }

    fn lab_request(network: &str, ip: &str) -> StartRequest {
        StartRequest {
            instance_id: "9".into(),
            spec: spec(),
            placement: Placement::Lab {
                network: network.into(),
                ip: ip.into(),
                gateway: None,
            },
            name: None,
            meta: Vec::new(),
        }
    }

    #[tokio::test]
    async fn lab_placement_needs_an_existing_lab_publishes_nothing_and_keeps_addresses_unique() {
        let rt = FakeRuntime::new();
        assert!(
            rt.start(&lab_request("lab-1", "10.77.0.10")).await.is_err(),
            "no such lab yet"
        );
        rt.ensure_lab("lab-1", "10.77.0.0/27").await.unwrap();
        rt.ensure_lab("lab-1", "10.77.0.0/27").await.unwrap();
        assert_eq!(
            rt.labs(),
            vec![("lab-1".to_owned(), "10.77.0.0/27".to_owned())]
        );

        let a = rt.start(&lab_request("lab-1", "10.77.0.10")).await.unwrap();
        assert_eq!(
            (a.ip.as_deref(), a.host_port),
            (Some("10.77.0.10"), 80),
            "the service port, not a published one"
        );
        assert!(
            rt.start(&lab_request("lab-1", "10.77.0.10")).await.is_err(),
            "address already taken"
        );
        let b = rt.start(&lab_request("lab-1", "10.77.0.11")).await.unwrap();

        assert!(
            rt.remove_lab("lab-1").await.is_err(),
            "containers are still in it"
        );
        rt.stop(&a.handle).await.unwrap();
        rt.stop(&b.handle).await.unwrap();
        rt.remove_lab("lab-1").await.unwrap();
        rt.remove_lab("lab-1").await.unwrap();
        assert!(rt.labs().is_empty());
        assert!(rt.start(&lab_request("lab-1", "10.77.0.10")).await.is_err());
    }

    #[tokio::test]
    async fn ports_are_distinct_and_failures_can_be_forced() {
        let rt = FakeRuntime::new();
        let req = StartRequest::new("1", spec());
        let a = rt.start(&req).await.unwrap();
        let b = rt.start(&req).await.unwrap();
        assert_ne!(a.host_port, b.host_port);
        rt.set_fail_starts(true);
        assert!(matches!(rt.start(&req).await, Err(RuntimeError::Failed(_))));
    }
}
