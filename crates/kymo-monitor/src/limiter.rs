use std::collections::{HashMap, VecDeque};

use crate::rule::Rule;

/// Denied requests older than this no longer count towards a ban.
pub const STRIKE_WINDOW_SECS: i64 = 600;
const MAX_TRACKED: usize = 50_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Throttle {
        retry_after_secs: i64,
    },
    /// The caller crossed the strike limit; `duration_secs` is None for a permanent ban.
    Ban {
        duration_secs: Option<i64>,
    },
}

#[derive(Default)]
struct Window {
    hits: VecDeque<i64>,
    strikes: VecDeque<i64>,
}

/// Sliding window counters per (rule, subject). Lives in memory; a restart forgets it, which
/// only ever makes the limiter more lenient for a moment.
#[derive(Default)]
pub struct Limiter {
    windows: HashMap<(i64, String), Window>,
}

impl Limiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts one request against `rule` for `subject`. Denied requests are not counted as
    /// hits, so a throttled caller recovers as soon as old hits leave the window.
    pub fn check(&mut self, rule: &Rule, subject: &str, now: i64) -> Decision {
        if self.windows.len() >= MAX_TRACKED {
            self.shed(now);
        }
        let window = self
            .windows
            .entry((rule.id, subject.to_owned()))
            .or_default();
        while window
            .hits
            .front()
            .is_some_and(|t| now >= t + rule.window_secs)
        {
            window.hits.pop_front();
        }
        if window.hits.len() < rule.max_requests as usize {
            window.hits.push_back(now);
            return Decision::Allow;
        }
        while window
            .strikes
            .front()
            .is_some_and(|t| now >= t + STRIKE_WINDOW_SECS)
        {
            window.strikes.pop_front();
        }
        window.strikes.push_back(now);
        if rule
            .strikes_to_ban
            .is_some_and(|n| window.strikes.len() >= n as usize)
        {
            window.hits.clear();
            window.strikes.clear();
            return Decision::Ban {
                duration_secs: (rule.ban_secs > 0).then_some(rule.ban_secs),
            };
        }
        let oldest = window.hits.front().copied().unwrap_or(now);
        Decision::Throttle {
            retry_after_secs: (oldest + rule.window_secs - now).max(1),
        }
    }

    /// Drops state that can no longer matter. Windows differ per rule, so this keeps only
    /// subjects with a hit or strike in the last strike window.
    fn shed(&mut self, now: i64) {
        self.windows.retain(|_, w| {
            w.hits.back().is_some_and(|t| now < t + STRIKE_WINDOW_SECS)
                || w.strikes
                    .back()
                    .is_some_and(|t| now < t + STRIKE_WINDOW_SECS)
        });
        if self.windows.len() >= MAX_TRACKED {
            self.windows.clear();
        }
    }

    pub fn tracked(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::Scope;

    fn rule(max: u32, window: i64, strikes: Option<u32>, ban: i64) -> Rule {
        Rule {
            id: 1,
            endpoint: "*".into(),
            max_requests: max,
            window_secs: window,
            scope: Scope::Ip,
            strikes_to_ban: strikes,
            ban_secs: ban,
        }
    }

    #[test]
    fn allows_up_to_the_limit_then_throttles_with_a_retry_time() {
        let r = rule(2, 10, None, 0);
        let mut l = Limiter::new();
        assert_eq!(l.check(&r, "a", 100), Decision::Allow);
        assert_eq!(l.check(&r, "a", 103), Decision::Allow);
        assert_eq!(
            l.check(&r, "a", 105),
            Decision::Throttle {
                retry_after_secs: 5
            }
        );
    }

    #[test]
    fn the_window_frees_exactly_when_the_oldest_hit_leaves() {
        let r = rule(1, 10, None, 0);
        let mut l = Limiter::new();
        assert_eq!(l.check(&r, "a", 100), Decision::Allow);
        assert!(matches!(l.check(&r, "a", 109), Decision::Throttle { .. }));
        assert_eq!(l.check(&r, "a", 110), Decision::Allow);
    }

    #[test]
    fn subjects_and_rules_are_counted_separately() {
        let r = rule(1, 60, None, 0);
        let mut other = rule(1, 60, None, 0);
        other.id = 2;
        let mut l = Limiter::new();
        assert_eq!(l.check(&r, "a", 0), Decision::Allow);
        assert_eq!(l.check(&r, "b", 0), Decision::Allow);
        assert_eq!(l.check(&other, "a", 0), Decision::Allow);
        assert!(matches!(l.check(&r, "a", 1), Decision::Throttle { .. }));
    }

    #[test]
    fn repeated_denials_become_a_ban_and_reset_the_counters() {
        let r = rule(1, 60, Some(3), 900);
        let mut l = Limiter::new();
        assert_eq!(l.check(&r, "a", 0), Decision::Allow);
        assert!(matches!(l.check(&r, "a", 1), Decision::Throttle { .. }));
        assert!(matches!(l.check(&r, "a", 2), Decision::Throttle { .. }));
        assert_eq!(
            l.check(&r, "a", 3),
            Decision::Ban {
                duration_secs: Some(900)
            }
        );
        assert_eq!(
            l.check(&r, "a", 4),
            Decision::Allow,
            "fresh start after the ban"
        );
    }

    #[test]
    fn zero_ban_secs_is_permanent_and_old_strikes_expire() {
        let r = rule(1, 5, Some(2), 0);
        let mut l = Limiter::new();
        assert_eq!(l.check(&r, "a", 0), Decision::Allow);
        assert!(matches!(l.check(&r, "a", 1), Decision::Throttle { .. }));
        // The first strike is older than ten minutes by now, so this is strike one again.
        assert!(matches!(
            l.check(&r, "a", STRIKE_WINDOW_SECS + 1),
            Decision::Allow
        ));
        assert!(matches!(
            l.check(&r, "a", STRIKE_WINDOW_SECS + 2),
            Decision::Throttle { .. }
        ));
        assert_eq!(
            l.check(&r, "a", STRIKE_WINDOW_SECS + 3),
            Decision::Ban {
                duration_secs: None
            }
        );
    }

    #[test]
    fn tracking_stays_bounded() {
        let r = rule(1, 60, None, 0);
        let mut l = Limiter::new();
        for i in 0..(MAX_TRACKED + 10) {
            l.check(&r, &i.to_string(), 0);
        }
        assert!(l.tracked() <= MAX_TRACKED);
    }
}
