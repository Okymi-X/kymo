//! Per player OpenVPN access to isolated challenge networks.
//!
//! `addressing`, `pki`, `profile` and `rules` are pure. `gateway` is the interface to the
//! container that runs OpenVPN, with an in memory fake and (feature `docker`) a Docker or
//! Podman implementation.

#[cfg(feature = "docker")]
pub mod docker;

mod addressing;
mod error;
mod gateway;
mod pki;
mod profile;
mod rules;

pub use addressing::{FIRST_INSTANCE_OFFSET, Ipv4Net, LabPool, TunnelPool};
pub use error::{GatewayError, VpnError};
pub use gateway::{FakeGateway, Gateway, GatewayConfig, GatewayStatus};
pub use pki::{Ca, CaMaterial, Issued, Revoked, generate_tls_crypt_key};
pub use profile::{
    ClientProfile, INNER_PORT, MANAGEMENT_PORT, MOUNT, client_config, client_profile, server_conf,
    valid_common_name, valid_remote_host,
};
pub use rules::{Allow, firewall_script};
