//! Docker runtime. Each instance gets its own bridge network, so two instances (even of one
//! team) cannot reach each other, and a container with no capabilities, no privilege
//! escalation, capped memory, CPU and processes. Everything is labelled so the reaper can
//! find containers a crashed server left behind.
//!
//! Not exercised against a live daemon in this repository's tests; the lifecycle rules it
//! serves are covered through `FakeRuntime`.

use std::collections::HashMap;

use async_trait::async_trait;
use bollard::Docker;
use bollard::models::{
    ContainerCreateBody, EndpointIpamConfig, EndpointSettings, HostConfig, Ipam, IpamConfig,
    NetworkCreateRequest, NetworkingConfig, PortBinding,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, KillContainerOptionsBuilder, ListContainersOptionsBuilder,
    RemoveContainerOptionsBuilder,
};

use crate::error::RuntimeError;
use crate::hardening::{ALLOWED_CAPS, Hardening};
use crate::runtime::{
    InstanceRuntime, Managed, Placement, RuntimeStatus, StartRequest, Started, VerifierSpec,
};
use crate::spec::InstanceSpec;

const LABEL_MANAGED: &str = "kymo.managed";
const LABEL_INSTANCE: &str = "kymo.instance";
/// Marks a Verifier container. It deliberately has no `kymo.managed` label, so the lab host does
/// not mistake it for an instance to adopt; it is found by this one.
const LABEL_VERIFIER: &str = "kymo.verifier";

pub struct DockerRuntime {
    docker: Docker,
    /// Address published ports bind to; 0.0.0.0 makes them reachable by players.
    bind_ip: String,
    podman: tokio::sync::OnceCell<bool>,
    /// What the `kymo.managed` label says. The platform's own runtime uses `1`; a lab host uses
    /// `lab`. Two Kymo components on one Docker daemon then never list, reap or name-clash with
    /// each other's containers and networks.
    scope: String,
    /// Per image: what the lab host's manifest relaxes. Empty on a platform's own runtime.
    hardening: std::sync::RwLock<HashMap<String, Hardening>>,
}

impl DockerRuntime {
    pub fn connect(bind_ip: impl Into<String>) -> Result<Self, RuntimeError> {
        Self::connect_scoped(bind_ip, "1")
    }

    /// Like `connect`, with its own label scope (letters, digits and dashes). A lab host uses
    /// `lab` so it can share a machine with a platform without either touching the other's
    /// containers.
    pub fn connect_scoped(bind_ip: impl Into<String>, scope: &str) -> Result<Self, RuntimeError> {
        if scope.is_empty() || !scope.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(RuntimeError::Failed("the scope is not valid".into()));
        }
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        Ok(Self {
            docker,
            bind_ip: bind_ip.into(),
            podman: tokio::sync::OnceCell::new(),
            scope: scope.to_owned(),
            hardening: std::sync::RwLock::new(HashMap::new()),
        })
    }

    /// Sets what each image may relax, by image reference. Called once by the lab host from its
    /// manifests. An entry that fails `Hardening::check` is ignored, so a bad value can only make
    /// a container stricter, never looser.
    pub fn set_hardening(&self, per_image: HashMap<String, Hardening>) {
        let valid = per_image
            .into_iter()
            .filter(|(_, h)| h.check().is_ok())
            .collect();
        *self.hardening.write().unwrap_or_else(|e| e.into_inner()) = valid;
    }

    fn hardening_for(&self, image: &str) -> Hardening {
        self.hardening
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(image)
            .cloned()
            .unwrap_or_default()
    }

    /// Podman needs `isolate` to keep its bridge networks apart; Docker keeps them apart by
    /// itself and may refuse an option it does not know.
    async fn is_podman(&self) -> bool {
        *self
            .podman
            .get_or_init(|| async {
                self.docker.version().await.is_ok_and(|v| {
                    v.components
                        .unwrap_or_default()
                        .iter()
                        .any(|c| c.name.to_lowercase().contains("podman"))
                })
            })
            .await
    }

    fn network_name(&self, instance_id: &str) -> String {
        if self.scope == "1" {
            format!("kymo-{instance_id}")
        } else {
            // Not `kymo-lab-N`: that is what the VPN calls its lab networks.
            format!("kymo-s-{}-{instance_id}", self.scope)
        }
    }

    fn labels(&self, instance_id: &str) -> HashMap<String, String> {
        HashMap::from([
            (LABEL_MANAGED.to_owned(), self.scope.clone()),
            (LABEL_INSTANCE.to_owned(), instance_id.to_owned()),
        ])
    }

    /// Labels for a request: the managed marker, the platform id, the platform's name for the
    /// instance and its ownership metadata, so what a host runs can be read from the host.
    fn request_labels(&self, request: &StartRequest) -> HashMap<String, String> {
        let mut labels = self.labels(&request.instance_id);
        if let Some(name) = &request.name {
            labels.insert("kymo.name".to_owned(), name.clone());
        }
        for (key, value) in &request.meta {
            if key.chars().all(|c| c.is_ascii_lowercase() || c == '_') && key.len() <= 32 {
                labels.insert(
                    format!("kymo.meta.{key}"),
                    value.chars().take(120).collect(),
                );
            }
        }
        labels
    }

    /// The container's name: the platform's, when it gave one and it is safe, else derived.
    fn container_name(&self, request: &StartRequest) -> String {
        match &request.name {
            Some(name) if valid_network_name(name) => name.clone(),
            _ => self.network_name(&request.instance_id),
        }
    }
}

/// Limits and hardening shared by every placement.
fn base_host_config(spec: &InstanceSpec, hardening: &Hardening) -> HostConfig {
    // Whatever was asked, only the allowed capabilities are ever added back.
    let add: Vec<String> = hardening
        .cap_add
        .iter()
        .filter(|c| ALLOWED_CAPS.contains(&c.as_str()))
        .cloned()
        .collect();
    HostConfig {
        memory: Some(i64::from(spec.memory_mb) * 1024 * 1024),
        memory_swap: Some(i64::from(spec.memory_mb) * 1024 * 1024),
        nano_cpus: Some(i64::from(spec.cpu_millis) * 1_000_000),
        pids_limit: Some(i64::from(spec.pids)),
        cap_drop: Some(vec!["ALL".into()]),
        cap_add: (!add.is_empty()).then_some(add),
        security_opt: hardening
            .no_new_privileges
            .then(|| vec!["no-new-privileges".to_owned()]),
        readonly_rootfs: Some(spec.read_only),
        privileged: Some(false),
        auto_remove: Some(false),
        ..Default::default()
    }
}

/// Names that end up in API paths and filters stay simple.
fn valid_network_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn failed(e: bollard::errors::Error) -> RuntimeError {
    RuntimeError::Failed(e.to_string())
}

fn is_missing(e: &bollard::errors::Error) -> bool {
    matches!(
        e,
        bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

impl DockerRuntime {
    async fn start_lab(
        &self,
        request: &StartRequest,
        network: &str,
        ip: &str,
    ) -> Result<Started, RuntimeError> {
        if !valid_network_name(network) || ip.parse::<std::net::Ipv4Addr>().is_err() {
            return Err(RuntimeError::Failed(
                "the lab network or address is not valid".into(),
            ));
        }
        let spec = &request.spec;
        let hardening = self.hardening_for(&spec.image);
        let mut host_config = base_host_config(spec, &hardening);
        host_config.network_mode = Some(network.to_owned());
        let mut labels = self.request_labels(request);
        labels.insert("kymo.lab".to_owned(), network.to_owned());
        let body = ContainerCreateBody {
            image: Some(spec.image.clone()),
            hostname: hardening.hostname.clone(),
            env: Some(spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
            labels: Some(labels),
            host_config: Some(host_config),
            networking_config: Some(NetworkingConfig {
                endpoints_config: Some(HashMap::from([(
                    network.to_owned(),
                    EndpointSettings {
                        ipam_config: Some(EndpointIpamConfig {
                            ipv4_address: Some(ip.to_owned()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )])),
            }),
            ..Default::default()
        };
        let name = self.container_name(request);
        let options = CreateContainerOptionsBuilder::new().name(&name).build();
        let created = self
            .docker
            .create_container(Some(options), body)
            .await
            .map_err(failed)?;
        if let Err(e) = self
            .docker
            .start_container(
                &created.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
        {
            let _ = self.stop(&created.id).await;
            return Err(failed(e));
        }
        Ok(Started::new(created.id, spec.port, Some(ip.to_owned())))
    }

    /// A Verifier next to an instance: no capabilities, read-only root, a small scratch space,
    /// the organizer's definition mounted read-only, and nothing but the lab network. The run
    /// description is handed over in the environment, which only the host's root can read.
    async fn start_verifier_container(&self, spec: &VerifierSpec) -> Result<String, RuntimeError> {
        if !valid_network_name(&spec.network)
            || !valid_network_name(&spec.name)
            || spec.ip.parse::<std::net::Ipv4Addr>().is_err()
        {
            return Err(RuntimeError::Failed(
                "the Verifier's network, name or address is not valid".into(),
            ));
        }
        let host_config = HostConfig {
            memory: Some(i64::from(spec.memory_mb) * 1024 * 1024),
            memory_swap: Some(i64::from(spec.memory_mb) * 1024 * 1024),
            nano_cpus: Some(i64::from(spec.cpu_millis) * 1_000_000),
            pids_limit: Some(128),
            cap_drop: Some(vec!["ALL".into()]),
            security_opt: Some(vec!["no-new-privileges".into()]),
            readonly_rootfs: Some(true),
            privileged: Some(false),
            auto_remove: Some(false),
            network_mode: Some(spec.network.clone()),
            binds: Some(vec![format!("{}:/verifier:ro", spec.definition_dir)]),
            tmpfs: Some(HashMap::from([(
                "/tmp".to_owned(),
                "rw,nosuid,size=64m".to_owned(),
            )])),
            ..Default::default()
        };
        let labels = HashMap::from([
            (LABEL_VERIFIER.to_owned(), self.scope.clone()),
            (
                LABEL_INSTANCE.to_owned(),
                format!("{}-verifier", spec.instance_id),
            ),
            ("kymo.verifier-of".to_owned(), spec.instance_id.clone()),
        ]);
        let body = ContainerCreateBody {
            image: Some(spec.image.clone()),
            env: Some(vec![
                format!("KYMO_VERIFIER_ADDR=0.0.0.0:{}", spec.port),
                format!("KYMO_VERIFIER_RUN_JSON={}", spec.run_json),
            ]),
            labels: Some(labels),
            host_config: Some(host_config),
            networking_config: Some(NetworkingConfig {
                endpoints_config: Some(HashMap::from([(
                    spec.network.clone(),
                    EndpointSettings {
                        ipam_config: Some(EndpointIpamConfig {
                            ipv4_address: Some(spec.ip.clone()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )])),
            }),
            ..Default::default()
        };
        let options = CreateContainerOptionsBuilder::new()
            .name(&spec.name)
            .build();
        let created = self
            .docker
            .create_container(Some(options), body)
            .await
            .map_err(failed)?;
        if let Err(e) = self
            .docker
            .start_container(
                &created.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
        {
            let _ = self.stop(&created.id).await;
            return Err(failed(e));
        }
        Ok(created.id)
    }

    async fn start_published(&self, request: &StartRequest) -> Result<Started, RuntimeError> {
        let spec = &request.spec;
        let id = &request.instance_id;
        let network = self.network_name(id);
        self.docker
            .create_network(NetworkCreateRequest {
                name: network.clone(),
                driver: Some("bridge".into()),
                labels: Some(self.labels(id)),
                ..Default::default()
            })
            .await
            .map_err(failed)?;

        let port_key = format!("{}/tcp", spec.port);
        let hardening = self.hardening_for(&spec.image);
        let mut host_config = base_host_config(spec, &hardening);
        host_config.network_mode = Some(network.clone());
        host_config.port_bindings = Some(HashMap::from([(
            port_key.clone(),
            Some(vec![PortBinding {
                host_ip: Some(self.bind_ip.clone()),
                host_port: Some(String::new()),
            }]),
        )]));
        let body = ContainerCreateBody {
            image: Some(spec.image.clone()),
            hostname: hardening.hostname.clone(),
            env: Some(spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
            exposed_ports: Some(HashMap::from([(port_key.clone(), HashMap::new())])),
            labels: Some(self.request_labels(request)),
            host_config: Some(host_config),
            ..Default::default()
        };
        let name = self.container_name(request);
        let options = CreateContainerOptionsBuilder::new().name(&name).build();
        let created = match self.docker.create_container(Some(options), body).await {
            Ok(created) => created,
            Err(e) => {
                let _ = self.docker.remove_network(&network).await;
                return Err(failed(e));
            }
        };
        if let Err(e) = self
            .docker
            .start_container(
                &created.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
        {
            let _ = self.stop(&created.id).await;
            return Err(failed(e));
        }
        let inspected = self
            .docker
            .inspect_container(
                &created.id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
            .map_err(failed)?;
        let host_port = inspected
            .network_settings
            .and_then(|n| n.ports)
            .and_then(|mut ports| ports.remove(&port_key))
            .flatten()
            .and_then(|bindings| bindings.into_iter().next())
            .and_then(|b| b.host_port)
            .and_then(|p| p.parse::<u16>().ok());
        match host_port {
            Some(host_port) => Ok(Started::new(created.id, host_port, None)),
            None => {
                let _ = self.stop(&created.id).await;
                Err(RuntimeError::Failed("no published port".into()))
            }
        }
    }
}

#[async_trait]
impl InstanceRuntime for DockerRuntime {
    async fn start(&self, request: &StartRequest) -> Result<Started, RuntimeError> {
        match &request.placement {
            Placement::Published => self.start_published(request).await,
            Placement::Lab { network, ip, .. } => self.start_lab(request, network, ip).await,
        }
    }

    async fn start_verifier(&self, spec: &VerifierSpec) -> Result<String, RuntimeError> {
        self.start_verifier_container(spec).await
    }

    async fn list_verifiers(&self) -> Result<Vec<Managed>, RuntimeError> {
        let filters = HashMap::from([(
            "label".to_owned(),
            vec![format!("{LABEL_VERIFIER}={}", self.scope)],
        )]);
        let options = ListContainersOptionsBuilder::new()
            .all(true)
            .filters(&filters)
            .build();
        let list = self
            .docker
            .list_containers(Some(options))
            .await
            .map_err(failed)?;
        Ok(list
            .into_iter()
            .filter_map(|c| {
                let id = c.id?;
                let of = c.labels?.get("kymo.verifier-of").cloned()?;
                Some(Managed {
                    handle: id,
                    instance_id: of,
                })
            })
            .collect())
    }

    async fn stop(&self, handle: &str) -> Result<(), RuntimeError> {
        // Read the instance id before the container is gone: it names the network.
        let network = match self
            .docker
            .inspect_container(
                handle,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
        {
            Ok(info) => info
                .config
                .and_then(|c| c.labels)
                .and_then(|l| l.get(LABEL_INSTANCE).cloned())
                .map(|id| self.network_name(&id)),
            Err(e) if is_missing(&e) => None,
            Err(e) => return Err(failed(e)),
        };
        // Instances are disposable, so skip the stop grace period: a service running as
        // PID 1 often ignores SIGTERM and would make every Stop wait ten seconds.
        let _ = self
            .docker
            .kill_container(
                handle,
                Some(KillContainerOptionsBuilder::new().signal("SIGKILL").build()),
            )
            .await;
        let options = RemoveContainerOptionsBuilder::new()
            .force(true)
            .v(true)
            .build();
        match self.docker.remove_container(handle, Some(options)).await {
            Ok(()) => {}
            Err(e) if is_missing(&e) => {}
            Err(e) => return Err(failed(e)),
        }
        if let Some(network) = network {
            match self.docker.remove_network(&network).await {
                Ok(()) => {}
                Err(e) if is_missing(&e) => {}
                Err(e) => return Err(failed(e)),
            }
        }
        Ok(())
    }

    async fn status(&self, handle: &str) -> Result<RuntimeStatus, RuntimeError> {
        match self
            .docker
            .inspect_container(
                handle,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
        {
            Ok(info) => Ok(if info.state.and_then(|s| s.running).unwrap_or(false) {
                RuntimeStatus::Running
            } else {
                RuntimeStatus::Exited
            }),
            Err(e) if is_missing(&e) => Ok(RuntimeStatus::Missing),
            Err(e) => Err(failed(e)),
        }
    }

    async fn list_managed(&self) -> Result<Vec<Managed>, RuntimeError> {
        let filters = HashMap::from([(
            "label".to_owned(),
            vec![format!("{LABEL_MANAGED}={}", self.scope)],
        )]);
        let options = ListContainersOptionsBuilder::new()
            .all(true)
            .filters(&filters)
            .build();
        let list = self
            .docker
            .list_containers(Some(options))
            .await
            .map_err(failed)?;
        Ok(list
            .into_iter()
            .filter_map(|c| {
                let id = c.id?;
                let instance = c.labels?.get(LABEL_INSTANCE).cloned()?;
                Some(Managed {
                    handle: id,
                    instance_id: instance,
                })
            })
            .collect())
    }

    async fn ensure_lab(&self, network: &str, subnet: &str) -> Result<(), RuntimeError> {
        if !valid_network_name(network)
            || !subnet
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '/')
        {
            return Err(RuntimeError::Failed(
                "the lab network or subnet is not valid".into(),
            ));
        }
        let mut options = HashMap::new();
        if self.is_podman().await {
            options.insert("isolate".to_owned(), "true".to_owned());
        }
        let labels = HashMap::from([
            (LABEL_MANAGED.to_owned(), self.scope.clone()),
            ("kymo.lab".to_owned(), network.to_owned()),
        ]);
        let created = self
            .docker
            .create_network(NetworkCreateRequest {
                name: network.to_owned(),
                driver: Some("bridge".into()),
                // No route out of the lab, and Docker or Podman add no outside NAT for it.
                internal: Some(true),
                ipam: Some(Ipam {
                    config: Some(vec![IpamConfig {
                        subnet: Some(subnet.to_owned()),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                options: Some(options),
                labels: Some(labels),
                ..Default::default()
            })
            .await;
        match created {
            Ok(_) => Ok(()),
            Err(e)
                if matches!(
                    &e,
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 409,
                        ..
                    }
                ) || e.to_string().contains("already exists") =>
            {
                Ok(())
            }
            Err(e) => Err(failed(e)),
        }
    }

    async fn remove_lab(&self, network: &str) -> Result<(), RuntimeError> {
        if !valid_network_name(network) {
            return Err(RuntimeError::Failed("the lab network is not valid".into()));
        }
        match self.docker.remove_network(network).await {
            Ok(()) => Ok(()),
            Err(e) if is_missing(&e) => Ok(()),
            Err(e) => Err(failed(e)),
        }
    }

    async fn reachable(&self, host_port: u16) -> bool {
        // Connect the way a player would: to the address the port is published on. A wildcard
        // address means any local address works, so use loopback.
        let ip = match self.bind_ip.as_str() {
            "0.0.0.0" | "" | "::" => "127.0.0.1",
            other => other,
        };
        let connect = tokio::net::TcpStream::connect((ip, host_port));
        matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), connect).await,
            Ok(Ok(_))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> InstanceSpec {
        InstanceSpec {
            image: "lab/box:1".into(),
            port: 22,
            cpu_millis: 500,
            memory_mb: 256,
            pids: 128,
            ttl_secs: 600,
            env: vec![],
            read_only: true,
        }
    }

    #[test]
    fn the_default_drops_every_capability_and_blocks_new_privileges() {
        let host = base_host_config(&spec(), &Hardening::default());
        assert_eq!(host.cap_drop, Some(vec!["ALL".to_owned()]));
        assert_eq!(host.cap_add, None);
        assert_eq!(
            host.security_opt,
            Some(vec!["no-new-privileges".to_owned()])
        );
        assert_eq!(host.privileged, Some(false));
        assert_eq!(host.readonly_rootfs, Some(true));
    }

    #[test]
    fn a_manifest_can_add_allowed_capabilities_back_and_allow_setuid() {
        let relaxed = Hardening {
            cap_add: vec!["SETUID".into(), "SETGID".into(), "SYS_CHROOT".into()],
            no_new_privileges: false,
            hostname: None,
        };
        assert!(relaxed.check().is_ok());
        assert!(relaxed.is_relaxed());
        let host = base_host_config(&spec(), &relaxed);
        // Everything is still dropped first; the allowed few are added back.
        assert_eq!(host.cap_drop, Some(vec!["ALL".to_owned()]));
        assert_eq!(host.cap_add.as_ref().map(Vec::len), Some(3));
        assert_eq!(host.security_opt, None);
        // Limits, the read-only root and the unprivileged flag are untouched.
        assert_eq!(host.pids_limit, Some(128));
        assert_eq!(host.privileged, Some(false));
        assert_eq!(host.readonly_rootfs, Some(true));
    }

    #[test]
    fn dangerous_capabilities_are_refused_and_never_reach_the_container() {
        for cap in [
            "SYS_ADMIN",
            "NET_ADMIN",
            "SYS_PTRACE",
            "SYS_MODULE",
            "DAC_READ_SEARCH",
            "SYS_RAWIO",
            "SYS_TIME",
            "ALL",
            "CAP_CHOWN",
            "chown",
        ] {
            let h = Hardening {
                cap_add: vec![cap.into()],
                no_new_privileges: true,
                hostname: None,
            };
            assert!(h.check().is_err(), "{cap} must be refused");
            // Even if a bad value got this far, the container would not get it.
            assert_eq!(base_host_config(&spec(), &h).cap_add, None, "{cap}");
        }
    }

    #[test]
    fn the_allowlist_is_dockers_default_set_and_holds_nothing_that_reads_the_host() {
        for cap in ALLOWED_CAPS {
            assert!(
                !cap.starts_with("SYS_ADMIN") && *cap != "SYS_PTRACE" && *cap != "DAC_READ_SEARCH"
            );
        }
        assert!(ALLOWED_CAPS.contains(&"SETUID") && ALLOWED_CAPS.contains(&"SYS_CHROOT"));
    }
}
