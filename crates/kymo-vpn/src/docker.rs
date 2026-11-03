//! Docker or Podman implementation of the gateway. Not exercised by the unit tests; the
//! `tests/gateway.rs` integration test runs it against a real daemon.

use std::collections::HashMap;

use async_trait::async_trait;
use bollard::Docker;
use bollard::models::{
    ContainerCreateBody, DeviceMapping, ExecConfig, HostConfig, NetworkConnectRequest,
    NetworkDisconnectRequest, PortBinding, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, InspectContainerOptions, StartContainerOptions,
};
use futures_util::StreamExt;

use crate::error::GatewayError;
use crate::gateway::{Gateway, GatewayConfig, GatewayStatus};
use crate::profile::{INNER_PORT, MANAGEMENT_PORT, MOUNT, valid_common_name};

pub struct DockerGateway {
    docker: Docker,
    name: tokio::sync::Mutex<Option<String>>,
}

impl DockerGateway {
    pub fn connect() -> Result<Self, GatewayError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| GatewayError::Unavailable(e.to_string()))?;
        Ok(Self {
            docker,
            name: tokio::sync::Mutex::new(None),
        })
    }

    async fn container(&self) -> Result<String, GatewayError> {
        self.name
            .lock()
            .await
            .clone()
            .ok_or_else(|| GatewayError::Failed("the gateway has not been started yet".into()))
    }

    /// Runs a command in the gateway and returns its exit code and combined output.
    async fn exec_code(&self, cmd: Vec<String>) -> Result<(Option<i64>, String), GatewayError> {
        let name = self.container().await?;
        let created = self
            .docker
            .create_exec(
                &name,
                ExecConfig {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(cmd),
                    ..Default::default()
                },
            )
            .await
            .map_err(failed)?;
        let started = self
            .docker
            .start_exec(&created.id, None)
            .await
            .map_err(failed)?;
        let mut text = String::new();
        if let bollard::exec::StartExecResults::Attached { mut output, .. } = started {
            while let Some(chunk) = output.next().await {
                if let Ok(chunk) = chunk {
                    text.push_str(&String::from_utf8_lossy(&chunk.into_bytes()));
                }
            }
        }
        let code = self
            .docker
            .inspect_exec(&created.id)
            .await
            .map_err(failed)?
            .exit_code;
        Ok((code, text))
    }

    /// Like `exec_code`, but a non zero exit is an error.
    async fn exec(&self, cmd: Vec<String>) -> Result<String, GatewayError> {
        let (code, text) = self.exec_code(cmd).await?;
        match code {
            Some(0) => Ok(text),
            other => Err(GatewayError::Failed(format!(
                "command exited with {other:?}: {}",
                text.trim()
            ))),
        }
    }
}

fn failed(e: bollard::errors::Error) -> GatewayError {
    GatewayError::Failed(e.to_string())
}

fn status_is(e: &bollard::errors::Error, codes: &[u16]) -> bool {
    matches!(e, bollard::errors::Error::DockerResponseServerError { status_code, .. } if codes.contains(status_code))
}

#[async_trait]
impl Gateway for DockerGateway {
    async fn ensure_running(&self, config: &GatewayConfig) -> Result<(), GatewayError> {
        *self.name.lock().await = Some(config.name.clone());
        let existing = self
            .docker
            .inspect_container(&config.name, None::<InspectContainerOptions>)
            .await;
        match existing {
            Ok(info) => {
                if info.state.and_then(|s| s.running).unwrap_or(false) {
                    return Ok(());
                }
            }
            Err(e) if status_is(&e, &[404]) => {
                let port_key = format!("{INNER_PORT}/udp");
                let dir = config.dir.to_str().ok_or_else(|| {
                    GatewayError::Failed("the VPN directory is not valid text".into())
                })?;
                let bind = if config.bind_options.is_empty() {
                    format!("{dir}:{MOUNT}:ro")
                } else {
                    format!("{dir}:{MOUNT}:ro,{}", config.bind_options)
                };
                let body = ContainerCreateBody {
                    image: Some(config.image.clone()),
                    exposed_ports: Some(HashMap::from([(port_key.clone(), HashMap::new())])),
                    labels: Some(HashMap::from([("kymo.gateway".to_owned(), "1".to_owned())])),
                    host_config: Some(HostConfig {
                        binds: Some(vec![bind]),
                        port_bindings: Some(HashMap::from([(
                            port_key,
                            Some(vec![PortBinding {
                                host_ip: Some(config.publish_ip.clone()),
                                host_port: Some(config.publish_port.to_string()),
                            }]),
                        )])),
                        cap_drop: Some(vec!["ALL".into()]),
                        // NET_ADMIN for the tunnel and firewall. DAC_READ_SEARCH because the mounted
                        // directory belongs to the Kymo service user and is private (0700): with
                        // all other capabilities dropped, even root in the container could not
                        // open it, and the entrypoint would report the config as missing.
                        cap_add: Some(vec!["NET_ADMIN".into(), "DAC_READ_SEARCH".into()]),
                        devices: Some(vec![DeviceMapping {
                            path_on_host: Some("/dev/net/tun".into()),
                            path_in_container: Some("/dev/net/tun".into()),
                            cgroup_permissions: Some("rwm".into()),
                        }]),
                        sysctls: Some(HashMap::from([(
                            "net.ipv4.ip_forward".to_owned(),
                            "1".to_owned(),
                        )])),
                        security_opt: Some(vec!["no-new-privileges".into()]),
                        memory: Some(512 * 1024 * 1024),
                        pids_limit: Some(512),
                        restart_policy: Some(RestartPolicy {
                            name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
                            maximum_retry_count: None,
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let options = CreateContainerOptionsBuilder::new()
                    .name(&config.name)
                    .build();
                self.docker
                    .create_container(Some(options), body)
                    .await
                    .map_err(|e| {
                        if status_is(&e, &[404]) {
                            GatewayError::Failed(format!(
                                "the gateway image {} does not exist; build it first (see the README)",
                                config.image
                            ))
                        } else {
                            failed(e)
                        }
                    })?;
            }
            Err(e) => return Err(failed(e)),
        }
        self.docker
            .start_container(&config.name, None::<StartContainerOptions>)
            .await
            .map_err(failed)
    }

    async fn attach(&self, network: &str) -> Result<(), GatewayError> {
        let name = self.container().await?;
        match self
            .docker
            .connect_network(
                network,
                NetworkConnectRequest {
                    container: Some(name),
                    endpoint_config: None,
                },
            )
            .await
        {
            Ok(()) => Ok(()),
            // Already connected: Docker answers 403, Podman 409 or 500 with a message.
            Err(e) if status_is(&e, &[403, 409]) || e.to_string().contains("already") => Ok(()),
            Err(e) => Err(failed(e)),
        }
    }

    async fn detach(&self, network: &str) -> Result<(), GatewayError> {
        let name = self.container().await?;
        match self
            .docker
            .disconnect_network(
                network,
                NetworkDisconnectRequest {
                    container: Some(name),
                    force: Some(true),
                },
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(e)
                if status_is(&e, &[404, 403])
                    || e.to_string().contains("not connected")
                    || e.to_string().contains("no such") =>
            {
                Ok(())
            }
            Err(e) => Err(failed(e)),
        }
    }

    async fn apply_rules(&self) -> Result<(), GatewayError> {
        self.exec(vec!["sh".into(), format!("{MOUNT}/rules.sh")])
            .await
            .map(|_| ())
    }

    async fn kick(&self, common_name: &str) -> Result<(), GatewayError> {
        if !valid_common_name(common_name) {
            return Err(GatewayError::Failed("not a valid common name".into()));
        }
        // The management console reads one command per line; `quit` ends the session.
        let script = format!(
            "(echo 'kill {common_name}'; sleep 1; echo quit) | nc 127.0.0.1 {MANAGEMENT_PORT}"
        );
        self.exec(vec!["sh".into(), "-c".into(), script])
            .await
            .map(|_| ())
    }

    async fn status(&self) -> Result<GatewayStatus, GatewayError> {
        let name = self.container().await?;
        let info = self
            .docker
            .inspect_container(&name, None::<InspectContainerOptions>)
            .await
            .map_err(failed)?;
        let running = info.state.and_then(|s| s.running).unwrap_or(false);
        let mut networks: Vec<String> = info
            .network_settings
            .and_then(|n| n.networks)
            .map(|n| n.into_keys().collect())
            .unwrap_or_default();
        networks.sort();
        Ok(GatewayStatus { running, networks })
    }

    async fn reachable(&self, ip: std::net::Ipv4Addr, port: u16) -> Result<bool, GatewayError> {
        // The address and port are typed, so nothing here can be steered by input. `nc` exits
        // 0 once the connection is made and closed (stdin is empty), non zero when refused or
        // when nothing answers within two seconds.
        let script = format!("nc -w 2 {ip} {port} </dev/null");
        let (code, _) = self
            .exec_code(vec!["sh".into(), "-c".into(), script])
            .await?;
        Ok(code == Some(0))
    }
}
