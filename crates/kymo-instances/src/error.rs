use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpecError {
    #[error("the image reference is not valid")]
    Image,
    #[error("the port must be between 1 and 65535")]
    Port,
    #[error("{field} must be between 1 and {max}")]
    Limit { field: &'static str, max: i64 },
    #[error("environment: {0}")]
    Env(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeError {
    /// The container platform cannot be reached.
    #[error("the container platform is unavailable: {0}")]
    Unavailable(String),
    #[error("the container platform refused the request: {0}")]
    Failed(String),
}
