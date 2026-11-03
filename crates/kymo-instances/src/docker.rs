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
use crate::runtime::{InstanceRuntime, Managed, Placement, RuntimeStatus, StartRequest, Started};
use crate::spec::InstanceSpec;

const LABEL_MANAGED: &str = "kymo.managed";
const LABEL_INSTANCE: &str = "kymo.instance";

pub struct DockerRuntime {
    docker: Docker,
    /// Address published ports bind to; 0.0.0.0 makes them reachable by players.
    bind_ip: String,
    podman: tokio::sync::OnceCell<bool>,
}

impl DockerRuntime {
    pub fn connect(bind_ip: impl Into<String>) -> Result<Self, RuntimeError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        Ok(Self {
            docker,
            bind_ip: bind_ip.into(),
            podman: tokio::sync::OnceCell::new(),
        })
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

    fn network_name(instance_id: &str) -> String {
        format!("kymo-{instance_id}")
    }

    fn labels(instance_id: &str) -> HashMap<String, String> {
        HashMap::from([
            (LABEL_MANAGED.to_owned(), "1".to_owned()),
            (LABEL_INSTANCE.to_owned(), instance_id.to_owned()),
        ])
    }
}

/// Limits and hardening shared by every placement.
fn base_host_config(spec: &InstanceSpec) -> HostConfig {
    HostConfig {
        memory: Some(i64::from(spec.memory_mb) * 1024 * 1024),
        memory_swap: Some(i64::from(spec.memory_mb) * 1024 * 1024),
        nano_cpus: Some(i64::from(spec.cpu_millis) * 1_000_000),
        pids_limit: Some(i64::from(spec.pids)),
        cap_drop: Some(vec!["ALL".into()]),
        security_opt: Some(vec!["no-new-privileges".into()]),
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
        let id = &request.instance_id;
        let mut host_config = base_host_config(spec);
        host_config.network_mode = Some(network.to_owned());
        let mut labels = Self::labels(id);
        labels.insert("kymo.lab".to_owned(), network.to_owned());
        let body = ContainerCreateBody {
            image: Some(spec.image.clone()),
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
        let name = Self::network_name(id);
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
        Ok(Started {
            handle: created.id,
            host_port: spec.port,
            ip: Some(ip.to_owned()),
        })
    }

    async fn start_published(&self, request: &StartRequest) -> Result<Started, RuntimeError> {
        let spec = &request.spec;
        let id = &request.instance_id;
        let network = Self::network_name(id);
        self.docker
            .create_network(NetworkCreateRequest {
                name: network.clone(),
                driver: Some("bridge".into()),
                labels: Some(Self::labels(id)),
                ..Default::default()
            })
            .await
            .map_err(failed)?;

        let port_key = format!("{}/tcp", spec.port);
        let mut host_config = base_host_config(spec);
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
            env: Some(spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
            exposed_ports: Some(HashMap::from([(port_key.clone(), HashMap::new())])),
            labels: Some(Self::labels(id)),
            host_config: Some(host_config),
            ..Default::default()
        };
        let name = Self::network_name(id);
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
            Some(host_port) => Ok(Started {
                handle: created.id,
                host_port,
                ip: None,
            }),
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
            Placement::Lab { network, ip } => self.start_lab(request, network, ip).await,
        }
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
                .map(|id| Self::network_name(&id)),
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
        let filters = HashMap::from([("label".to_owned(), vec![format!("{LABEL_MANAGED}=1")])]);
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
            (LABEL_MANAGED.to_owned(), "1".to_owned()),
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
