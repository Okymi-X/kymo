//! The container that runs OpenVPN. The platform writes its config into a directory that is
//! mounted read only, then asks the platform to start the container, attach it to lab
//! networks, run the generated firewall script, and disconnect players.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::GatewayError;

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Container name, also how the platform finds it again after a restart.
    pub name: String,
    pub image: String,
    /// Host directory that becomes `/vpn` inside the container.
    pub dir: PathBuf,
    /// Address and UDP port on the host that players connect to.
    pub publish_ip: String,
    pub publish_port: u16,
    /// Extra bind mount options such as `z` on SELinux hosts.
    pub bind_options: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayStatus {
    pub running: bool,
    /// Every network the gateway is attached to, including the one players connect through.
    pub networks: Vec<String>,
}

#[async_trait]
pub trait Gateway: Send + Sync {
    /// Creates the container if needed and makes sure it is running.
    async fn ensure_running(&self, config: &GatewayConfig) -> Result<(), GatewayError>;
    /// Joins the gateway to a lab network. Joining twice is fine.
    async fn attach(&self, network: &str) -> Result<(), GatewayError>;
    /// Leaves a lab network. A network the gateway is not on is fine.
    async fn detach(&self, network: &str) -> Result<(), GatewayError>;
    /// Runs the generated firewall script (`/vpn/rules.sh`) inside the container.
    async fn apply_rules(&self) -> Result<(), GatewayError>;
    /// Ends a connected player's session. `common_name` must pass `valid_common_name`.
    async fn kick(&self, common_name: &str) -> Result<(), GatewayError>;
    async fn status(&self) -> Result<GatewayStatus, GatewayError>;
    /// Whether something accepts a TCP connection at `ip:port`, tried from inside the gateway,
    /// which is attached to every lab. This is how an instance in a private lab is checked.
    async fn reachable(&self, ip: std::net::Ipv4Addr, port: u16) -> Result<bool, GatewayError>;
}

#[derive(Default)]
struct FakeState {
    running: bool,
    networks: BTreeSet<String>,
    rules_runs: usize,
    kicked: Vec<String>,
    not_reachable: bool,
    fail: bool,
}

/// An in memory gateway for tests.
#[derive(Default)]
pub struct FakeGateway {
    inner: Mutex<FakeState>,
}

impl FakeGateway {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Makes `reachable` answer false, as a service that is not listening yet.
    pub fn set_unreachable(&self, unreachable: bool) {
        self.lock().not_reachable = unreachable;
    }

    /// Makes every call fail, as a stopped container platform would.
    pub fn set_fail(&self, fail: bool) {
        self.lock().fail = fail;
    }

    pub fn networks(&self) -> Vec<String> {
        self.lock().networks.iter().cloned().collect()
    }

    pub fn rules_runs(&self) -> usize {
        self.lock().rules_runs
    }

    pub fn kicked(&self) -> Vec<String> {
        self.lock().kicked.clone()
    }

    fn check(&self) -> Result<std::sync::MutexGuard<'_, FakeState>, GatewayError> {
        let state = self.lock();
        if state.fail {
            Err(GatewayError::Unavailable("forced failure".into()))
        } else {
            Ok(state)
        }
    }
}

#[async_trait]
impl Gateway for FakeGateway {
    async fn ensure_running(&self, _: &GatewayConfig) -> Result<(), GatewayError> {
        self.check()?.running = true;
        Ok(())
    }

    async fn attach(&self, network: &str) -> Result<(), GatewayError> {
        self.check()?.networks.insert(network.to_owned());
        Ok(())
    }

    async fn detach(&self, network: &str) -> Result<(), GatewayError> {
        self.check()?.networks.remove(network);
        Ok(())
    }

    async fn apply_rules(&self) -> Result<(), GatewayError> {
        self.check()?.rules_runs += 1;
        Ok(())
    }

    async fn kick(&self, common_name: &str) -> Result<(), GatewayError> {
        self.check()?.kicked.push(common_name.to_owned());
        Ok(())
    }

    async fn reachable(&self, _: std::net::Ipv4Addr, _: u16) -> Result<bool, GatewayError> {
        Ok(!self.check()?.not_reachable)
    }

    async fn status(&self) -> Result<GatewayStatus, GatewayError> {
        let state = self.check()?;
        Ok(GatewayStatus {
            running: state.running,
            networks: state.networks.iter().cloned().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> GatewayConfig {
        GatewayConfig {
            name: "gw".into(),
            image: "img".into(),
            dir: "/tmp/x".into(),
            publish_ip: "0.0.0.0".into(),
            publish_port: 1194,
            bind_options: String::new(),
        }
    }

    #[tokio::test]
    async fn the_fake_tracks_attachments_and_is_idempotent() {
        let gw = FakeGateway::new();
        gw.ensure_running(&config()).await.unwrap();
        gw.attach("a").await.unwrap();
        gw.attach("a").await.unwrap();
        gw.attach("b").await.unwrap();
        gw.detach("c").await.unwrap();
        gw.detach("a").await.unwrap();
        assert_eq!(
            gw.status().await.unwrap(),
            GatewayStatus {
                running: true,
                networks: vec!["b".into()]
            }
        );
        gw.apply_rules().await.unwrap();
        gw.kick("u1").await.unwrap();
        assert_eq!((gw.rules_runs(), gw.kicked()), (1, vec!["u1".to_owned()]));
    }

    #[tokio::test]
    async fn the_fake_can_simulate_an_unreachable_platform() {
        let gw = FakeGateway::new();
        gw.set_fail(true);
        assert!(matches!(
            gw.attach("a").await,
            Err(GatewayError::Unavailable(_))
        ));
        assert!(gw.status().await.is_err());
        gw.set_fail(false);
        assert!(gw.attach("a").await.is_ok());
    }
}
