use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VpnError {
    #[error("invalid address or network: {0}")]
    Address(String),
    #[error("the lab pool is full")]
    PoolFull,
    #[error("certificate error: {0}")]
    Certificate(String),
    #[error("{0}")]
    Profile(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GatewayError {
    /// The container platform cannot be reached.
    #[error("the container platform is unavailable: {0}")]
    Unavailable(String),
    #[error("the gateway failed: {0}")]
    Failed(String),
}
