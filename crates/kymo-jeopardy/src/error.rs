//! Typed errors for configuration validation.

use thiserror::Error;

/// Invalid flag configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FlagError {
    #[error("pattern is longer than {max} characters")]
    PatternTooLong { max: usize },
    #[error("pattern is not a valid regular expression: {0}")]
    InvalidPattern(String),
}

/// Invalid scoring configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScoringError {
    #[error("initial points must not be negative")]
    NegativeInitial,
    #[error("minimum points must be between 0 and the initial points")]
    BadMinimum,
    #[error("decay must be at least 1")]
    ZeroDecay,
    #[error("static points must not be negative")]
    NegativePoints,
    #[error("hint cost must not be negative")]
    NegativeHintCost,
}
