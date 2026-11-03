//! The Kymo lab host and its client.
//!
//! A lab host is a machine that runs challenge containers for a Kymo platform and nothing else.
//! Keeping it apart means a challenge that is broken into cannot reach the platform's database,
//! accounts or flags. The platform talks to it over an authenticated, encrypted API:
//! [`client::RemoteRuntime`] makes a lab host look like any other `InstanceRuntime`.

pub mod client;
pub mod manifest;
pub mod naming;
pub mod protocol;

#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod tls;
#[cfg(feature = "server")]
pub mod vm;

pub use client::RemoteRuntime;
pub use manifest::Manifest;
pub use protocol::*;
