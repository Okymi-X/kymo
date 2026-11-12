//! Pure monitoring rules: which rule applies to an endpoint, the sliding window limiter with
//! strike based ban escalation, and ban expiry. No IO and no clock: time is passed in.

mod ban;
mod detect;
mod limiter;
mod rule;

pub use ban::{BanKind, expiry, is_active, parse_subject};
pub use detect::{
    Config as DetectorConfig, Hit, Kind as DetectorKind, Sample, default_config, evaluate,
    scanner_in, signature_in,
};
pub use limiter::{Decision, Limiter, STRIKE_WINDOW_SECS};
pub use rule::{Rule, RuleError, Scope};
