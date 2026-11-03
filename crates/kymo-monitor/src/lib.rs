//! Pure monitoring rules: which rule applies to an endpoint, the sliding window limiter with
//! strike based ban escalation, and ban expiry. No IO and no clock: time is passed in.

mod ban;
mod limiter;
mod rule;

pub use ban::{BanKind, expiry, is_active, parse_subject};
pub use limiter::{Decision, Limiter, STRIKE_WINDOW_SECS};
pub use rule::{Rule, RuleError, Scope};
