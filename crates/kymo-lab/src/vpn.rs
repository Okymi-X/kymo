//! The lab host's own VPN gateway. The platform keeps the authority (certificates, who may
//! connect, which lab they may reach) and pushes the result here; this module writes the files
//! where the gateway container reads them, and does what the platform asks of the container:
//! start it, join it to lab networks, run the firewall script, end a session.
//!
//! The lab host trusts the platform's token but still checks every value it is given, so a
//! mistaken or hostile request can neither write outside the gateway directory nor create a
//! network outside the address range set for this lab host.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kymo_instances::{InstanceRuntime, RuntimeError};
use kymo_vpn::{Gateway, GatewayConfig, GatewayError, Ipv4Net, valid_common_name};

use crate::protocol::{GatewayState, VpnFiles, VpnInfo};

/// Files the platform may write, besides one per player under `ccd/`.
const FILES: &[&str] = &[
    "pki/ca.crt",
    "pki/server.crt",
    "pki/server.key",
    "pki/ta.key",
    "pki/crl.pem",
    "server.conf",
    "rules.sh",
];

pub struct LabVpn {
    gateway: Arc<dyn Gateway>,
    runtime: Arc<dyn InstanceRuntime>,
    config: GatewayConfig,
    /// What players are told to connect to.
    host: String,
    pool: Ipv4Net,
}

impl LabVpn {
    pub fn new(
        gateway: Arc<dyn Gateway>,
        runtime: Arc<dyn InstanceRuntime>,
        config: GatewayConfig,
        host: String,
        pool: Ipv4Net,
    ) -> Self {
        Self {
            gateway,
            runtime,
            config,
            host,
            pool,
        }
    }

    pub fn info(&self) -> VpnInfo {
        VpnInfo {
            host: self.host.clone(),
            port: self.config.publish_port,
            pool: self.pool.to_string(),
        }
    }

    pub fn pool(&self) -> Ipv4Net {
        self.pool
    }

    /// A lab network the platform may ask for: `kymo-lab-` and a number.
    pub fn valid_network(name: &str) -> bool {
        name.strip_prefix("kymo-lab-")
            .is_some_and(|n| !n.is_empty() && n.len() <= 6 && n.chars().all(|c| c.is_ascii_digit()))
    }

    /// Whether an address lies inside the range this lab host gives to lab networks.
    pub fn in_pool(&self, ip: Ipv4Addr) -> bool {
        self.pool.contains(ip)
    }

    /// Replaces the gateway's files. The write is a temporary file and a rename, so the gateway
    /// never reads half of one.
    pub fn write_files(&self, files: &VpnFiles) -> Result<(), String> {
        let dir = &self.config.dir;
        let mut ccd = BTreeSet::new();
        for (path, file) in &files.files {
            let target = if let Some(name) = path.strip_prefix("ccd/") {
                if !valid_common_name(name) {
                    return Err(format!("{path} is not a valid player file"));
                }
                ccd.insert(name.to_owned());
                dir.join("ccd").join(name)
            } else if FILES.contains(&path.as_str()) {
                dir.join(path)
            } else {
                return Err(format!("{path} is not a file the gateway reads"));
            };
            if file.content.len() > 256 * 1024 {
                return Err(format!("{path} is too large"));
            }
            write_atomic(&target, &file.content, file.secret)
                .map_err(|e| format!("{path}: {e}"))?;
        }
        // A player no longer listed may no longer connect.
        let ccd_dir = dir.join("ccd");
        if let Ok(entries) = std::fs::read_dir(&ccd_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !ccd.contains(&name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    pub async fn ensure(&self) -> Result<(), GatewayError> {
        self.gateway.ensure_running(&self.config).await
    }

    pub async fn status(&self) -> Result<GatewayState, GatewayError> {
        let s = self.gateway.status().await?;
        Ok(GatewayState {
            running: s.running,
            networks: s.networks,
        })
    }

    pub async fn attach(&self, network: &str) -> Result<(), GatewayError> {
        self.gateway.attach(network).await
    }

    pub async fn detach(&self, network: &str) -> Result<(), GatewayError> {
        self.gateway.detach(network).await
    }

    pub async fn rules(&self) -> Result<(), GatewayError> {
        self.gateway.apply_rules().await
    }

    pub async fn kick(&self, common_name: &str) -> Result<(), GatewayError> {
        self.gateway.kick(common_name).await
    }

    pub async fn reachable(&self, ip: Ipv4Addr, port: u16) -> Result<bool, GatewayError> {
        self.gateway.reachable(ip, port).await
    }

    /// Creates a private lab network. The subnet must lie inside this lab host's range.
    pub async fn ensure_lab(&self, network: &str, subnet: &str) -> Result<(), String> {
        if !Self::valid_network(network) {
            return Err("the lab network name is not valid".into());
        }
        let net: Ipv4Net = subnet
            .parse()
            .map_err(|_| "the subnet is not valid".to_owned())?;
        if !self.pool.contains(net.network())
            || !self.pool.contains(
                net.host(u32::try_from(net.size().saturating_sub(1)).unwrap_or(0))
                    .unwrap_or(net.network()),
            )
        {
            return Err(format!(
                "the subnet {net} is outside this lab host's VPN range {}",
                self.pool
            ));
        }
        self.runtime
            .ensure_lab(network, subnet)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn remove_lab(&self, network: &str) -> Result<(), RuntimeError> {
        if !Self::valid_network(network) {
            return Err(RuntimeError::Failed(
                "the lab network name is not valid".into(),
            ));
        }
        // The gateway must leave first or the network is "in use".
        let _ = self.gateway.detach(network).await;
        self.runtime.remove_lab(network).await
    }
}

fn write_atomic(path: &Path, text: &str, secret: bool) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp: PathBuf = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if secret { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = secret;
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::VpnFile;
    use kymo_instances::FakeRuntime;
    use kymo_vpn::FakeGateway;

    fn vpn(dir: &Path) -> LabVpn {
        LabVpn::new(
            Arc::new(FakeGateway::new()),
            Arc::new(FakeRuntime::new()),
            GatewayConfig {
                name: "gw".into(),
                image: "img".into(),
                dir: dir.to_path_buf(),
                publish_ip: "0.0.0.0".into(),
                publish_port: 1194,
                bind_options: String::new(),
            },
            "lab.example".into(),
            "10.77.0.0/16".parse().unwrap(),
        )
    }

    fn files(entries: &[(&str, &str)]) -> VpnFiles {
        VpnFiles {
            files: entries
                .iter()
                .map(|(p, c)| {
                    (
                        (*p).to_owned(),
                        VpnFile {
                            content: (*c).to_owned(),
                            secret: false,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn writes_listed_files_and_drops_players_who_are_not_listed() {
        let dir = tempfile::tempdir().unwrap();
        let v = vpn(dir.path());
        v.write_files(&files(&[
            ("server.conf", "a"),
            ("ccd/u1", "x"),
            ("ccd/u2", "y"),
        ]))
        .unwrap();
        assert!(dir.path().join("ccd/u2").exists());
        v.write_files(&files(&[("ccd/u1", "x")])).unwrap();
        assert!(dir.path().join("ccd/u1").exists());
        assert!(!dir.path().join("ccd/u2").exists());
    }

    #[test]
    fn refuses_paths_outside_the_gateway_directory() {
        let dir = tempfile::tempdir().unwrap();
        let v = vpn(dir.path());
        for bad in [
            "../evil",
            "/etc/passwd",
            "ccd/../x",
            "ccd/U1",
            "other.conf",
            "ccd/",
        ] {
            assert!(v.write_files(&files(&[(bad, "x")])).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn lab_networks_must_be_named_and_placed_inside_the_range() {
        let dir = tempfile::tempdir().unwrap();
        let v = vpn(dir.path());
        assert!(v.ensure_lab("kymo-lab-3", "10.77.0.96/27").await.is_ok());
        assert!(v.ensure_lab("bridge", "10.77.0.96/27").await.is_err());
        assert!(v.ensure_lab("kymo-lab-3", "192.168.0.0/27").await.is_err());
        assert!(v.ensure_lab("kymo-lab-3", "10.0.0.0/8").await.is_err());
        assert!(v.ensure_lab("kymo-lab-3", "nonsense").await.is_err());
    }
}
