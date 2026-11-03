//! Submission rate limit math. Pure: the caller supplies the clock and
//! persists [`RateState`] per (participant, challenge) or per participant.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// At most `max_attempts` attempts inside any sliding window of
/// `window_secs`. Exceeding it locks the key out for `lockout_secs`
/// (0 means no lockout: just wait for the window to slide).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateRule {
    pub max_attempts: u32,
    pub window_secs: i64,
    pub lockout_secs: i64,
}

/// A submission was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("rate limited, retry in {retry_after_secs} seconds")]
pub struct RateDenied {
    pub retry_after_secs: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateState {
    attempts: VecDeque<i64>,
    locked_until: Option<i64>,
}

impl RateState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempts currently inside the window ending at `now`.
    pub fn attempts_in_window(&self, rule: &RateRule, now: i64) -> usize {
        let cutoff = now.saturating_sub(rule.window_secs);
        self.attempts.iter().filter(|t| **t > cutoff).count()
    }

    /// Records an attempt at `now` if allowed. A denied call records nothing.
    ///
    /// An attempt at `t` counts while `now < t + window_secs`; the window is
    /// half open, so it frees up exactly `window_secs` later. A lockout lasts
    /// until `now + lockout_secs` (exclusive) from the denied call.
    pub fn check_and_record(&mut self, rule: &RateRule, now: i64) -> Result<(), RateDenied> {
        if let Some(until) = self.locked_until {
            if now < until {
                return Err(RateDenied {
                    retry_after_secs: until - now,
                });
            }
            self.locked_until = None;
        }
        let cutoff = now.saturating_sub(rule.window_secs);
        while self.attempts.front().is_some_and(|t| *t <= cutoff) {
            self.attempts.pop_front();
        }
        if self.attempts.len() >= rule.max_attempts as usize {
            if rule.lockout_secs > 0 {
                self.locked_until = Some(now.saturating_add(rule.lockout_secs));
                return Err(RateDenied {
                    retry_after_secs: rule.lockout_secs,
                });
            }
            let retry = match self.attempts.front() {
                Some(oldest) => oldest.saturating_add(rule.window_secs) - now,
                None => rule.window_secs,
            };
            return Err(RateDenied {
                retry_after_secs: retry.max(1),
            });
        }
        self.attempts.push_back(now);
        Ok(())
    }
}

/// The challenge has used all its allowed attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("attempt limit of {max} reached")]
pub struct AttemptCapReached {
    pub max: u32,
}

/// Per challenge cap on attempts for one participant. `None` is unlimited.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptCap {
    pub max: Option<u32>,
}

impl AttemptCap {
    /// `used` is the number of counted attempts so far (bad format excluded).
    pub fn check(&self, used: u32) -> Result<(), AttemptCapReached> {
        match self.max {
            Some(max) if used >= max => Err(AttemptCapReached { max }),
            _ => Ok(()),
        }
    }

    pub fn remaining(&self, used: u32) -> Option<u32> {
        self.max.map(|m| m.saturating_sub(used))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: RateRule = RateRule {
        max_attempts: 3,
        window_secs: 60,
        lockout_secs: 0,
    };

    #[test]
    fn allows_up_to_max() {
        let mut s = RateState::new();
        for t in [0, 1, 2] {
            assert!(s.check_and_record(&RULE, t).is_ok());
        }
        assert_eq!(
            s.check_and_record(&RULE, 3),
            Err(RateDenied {
                retry_after_secs: 57
            })
        );
    }

    #[test]
    fn window_slides_at_exact_boundary() {
        let mut s = RateState::new();
        for t in [0, 10, 20] {
            s.check_and_record(&RULE, t).unwrap();
        }
        // attempt at 0 still counts at 59, gone at 60
        assert_eq!(
            s.check_and_record(&RULE, 59),
            Err(RateDenied {
                retry_after_secs: 1
            })
        );
        assert!(s.check_and_record(&RULE, 60).is_ok());
        // now 10, 20, 60 inside; 10 frees at 70
        assert_eq!(
            s.check_and_record(&RULE, 69),
            Err(RateDenied {
                retry_after_secs: 1
            })
        );
        assert!(s.check_and_record(&RULE, 70).is_ok());
    }

    #[test]
    fn denied_calls_are_not_recorded() {
        let mut s = RateState::new();
        for t in [0, 1, 2] {
            s.check_and_record(&RULE, t).unwrap();
        }
        for t in 3..50 {
            assert!(s.check_and_record(&RULE, t).is_err());
        }
        assert_eq!(s.attempts_in_window(&RULE, 49), 3);
        assert!(s.check_and_record(&RULE, 60).is_ok());
    }

    #[test]
    fn lockout_boundaries() {
        let rule = RateRule {
            max_attempts: 2,
            window_secs: 10,
            lockout_secs: 300,
        };
        let mut s = RateState::new();
        s.check_and_record(&rule, 0).unwrap();
        s.check_and_record(&rule, 1).unwrap();
        assert_eq!(
            s.check_and_record(&rule, 2),
            Err(RateDenied {
                retry_after_secs: 300
            })
        );
        // locked until 302 even though the window has long slid
        assert_eq!(
            s.check_and_record(&rule, 100),
            Err(RateDenied {
                retry_after_secs: 202
            })
        );
        assert_eq!(
            s.check_and_record(&rule, 301),
            Err(RateDenied {
                retry_after_secs: 1
            })
        );
        assert!(s.check_and_record(&rule, 302).is_ok());
    }

    #[test]
    fn zero_max_always_denies() {
        let rule = RateRule {
            max_attempts: 0,
            window_secs: 30,
            lockout_secs: 0,
        };
        let mut s = RateState::new();
        assert_eq!(
            s.check_and_record(&rule, 5),
            Err(RateDenied {
                retry_after_secs: 30
            })
        );
    }

    #[test]
    fn attempt_cap() {
        let cap = AttemptCap { max: Some(3) };
        assert!(cap.check(2).is_ok());
        assert_eq!(cap.check(3), Err(AttemptCapReached { max: 3 }));
        assert_eq!(cap.remaining(1), Some(2));
        assert_eq!(cap.remaining(9), Some(0));
        let none = AttemptCap::default();
        assert!(none.check(u32::MAX).is_ok());
        assert_eq!(none.remaining(5), None);
    }
}
