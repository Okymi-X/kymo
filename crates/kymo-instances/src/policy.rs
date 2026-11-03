//! Who may start, restart or stop an instance right now. Pure: the caller supplies the
//! record, counters and the time.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    Starting,
    Running,
    Stopped,
    Expired,
    Failed,
}

impl InstanceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Expired => "expired",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "starting" => Self::Starting,
            "running" => Self::Running,
            "stopped" => Self::Stopped,
            "expired" => Self::Expired,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    /// Holds a container (or is about to).
    pub fn is_live(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceRecord {
    pub state: InstanceState,
    pub started_at: i64,
    pub expires_at: i64,
}

/// A stored Running instance whose time is up reads as Expired even before the reaper has
/// stopped its container.
pub fn effective_state(record: &InstanceRecord, now: i64) -> InstanceState {
    if record.state.is_live() && now >= record.expires_at {
        InstanceState::Expired
    } else {
        record.state
    }
}

pub fn expires_at(started_at: i64, ttl_secs: i64) -> i64 {
    started_at.saturating_add(ttl_secs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Minimum seconds between a start and the next restart.
    pub restart_cooldown_secs: i64,
    /// Starts plus restarts one participant may use per challenge.
    pub max_starts: u32,
    /// Live instances one participant may hold at once across challenges.
    pub max_concurrent: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            restart_cooldown_secs: 30,
            max_starts: 5,
            max_concurrent: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Restart,
    Stop,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Denied {
    #[error("an instance is already running")]
    AlreadyRunning,
    #[error("there is no running instance")]
    NotRunning,
    #[error("the instance has expired")]
    Expired,
    #[error("wait {retry_after_secs} seconds before restarting")]
    Cooldown { retry_after_secs: i64 },
    #[error("the limit of {max} starts for this challenge is used")]
    StartLimit { max: u32 },
    #[error("only {max} instance(s) may run at once")]
    Concurrency { max: u32 },
}

/// Counters the caller reads from storage.
#[derive(Debug, Clone, Copy, Default)]
pub struct Usage {
    /// Starts and restarts already used on this challenge.
    pub starts_used: u32,
    /// Live instances this participant holds on other challenges.
    pub live_elsewhere: u32,
}

pub fn authorize(
    policy: &Policy,
    current: Option<&InstanceRecord>,
    usage: Usage,
    action: Action,
    now: i64,
) -> Result<(), Denied> {
    let state = current.map(|r| effective_state(r, now));
    match action {
        Action::Start => {
            if state.is_some_and(InstanceState::is_live) {
                return Err(Denied::AlreadyRunning);
            }
            check_budget(policy, usage, true)
        }
        Action::Restart => {
            let record = running(current, state)?;
            let ready = record
                .started_at
                .saturating_add(policy.restart_cooldown_secs);
            if now < ready {
                return Err(Denied::Cooldown {
                    retry_after_secs: ready - now,
                });
            }
            check_budget(policy, usage, false)
        }
        Action::Stop => running(current, state).map(|_| ()),
    }
}

fn running(
    current: Option<&InstanceRecord>,
    state: Option<InstanceState>,
) -> Result<&InstanceRecord, Denied> {
    match (current, state) {
        (Some(_), Some(InstanceState::Expired)) => Err(Denied::Expired),
        (Some(record), Some(s)) if s.is_live() => Ok(record),
        _ => Err(Denied::NotRunning),
    }
}

fn check_budget(policy: &Policy, usage: Usage, starting_new: bool) -> Result<(), Denied> {
    if usage.starts_used >= policy.max_starts {
        return Err(Denied::StartLimit {
            max: policy.max_starts,
        });
    }
    if starting_new && usage.live_elsewhere >= policy.max_concurrent {
        return Err(Denied::Concurrency {
            max: policy.max_concurrent,
        });
    }
    Ok(())
}

/// Live records whose time is up, for the reaper. `grace_secs` leaves a short delay so a
/// player finishing a request is not cut off to the second.
pub fn due_for_reaping<T: Copy>(
    records: &[(T, InstanceRecord)],
    now: i64,
    grace_secs: i64,
) -> Vec<T> {
    records
        .iter()
        .filter(|(_, r)| r.state.is_live() && now >= r.expires_at.saturating_add(grace_secs))
        .map(|(id, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running_at(started: i64) -> InstanceRecord {
        InstanceRecord {
            state: InstanceState::Running,
            started_at: started,
            expires_at: started + 1000,
        }
    }

    #[test]
    fn running_past_expiry_reads_as_expired_exactly_at_the_deadline() {
        let r = running_at(0);
        assert_eq!(effective_state(&r, 999), InstanceState::Running);
        assert_eq!(effective_state(&r, 1000), InstanceState::Expired);
        let stopped = InstanceRecord {
            state: InstanceState::Stopped,
            ..r
        };
        assert_eq!(effective_state(&stopped, 5000), InstanceState::Stopped);
    }

    #[test]
    fn start_needs_no_live_instance_and_budget() {
        let p = Policy::default();
        let none = Usage::default();
        assert_eq!(authorize(&p, None, none, Action::Start, 0), Ok(()));
        assert_eq!(
            authorize(&p, Some(&running_at(0)), none, Action::Start, 10),
            Err(Denied::AlreadyRunning)
        );
        let used = Usage {
            starts_used: 5,
            live_elsewhere: 0,
        };
        assert_eq!(
            authorize(&p, None, used, Action::Start, 0),
            Err(Denied::StartLimit { max: 5 })
        );
        let busy = Usage {
            starts_used: 0,
            live_elsewhere: 1,
        };
        assert_eq!(
            authorize(&p, None, busy, Action::Start, 0),
            Err(Denied::Concurrency { max: 1 })
        );
    }

    #[test]
    fn an_expired_instance_can_be_replaced_but_not_restarted_or_stopped() {
        let p = Policy::default();
        let r = running_at(0);
        let u = Usage::default();
        assert_eq!(authorize(&p, Some(&r), u, Action::Start, 2000), Ok(()));
        assert_eq!(
            authorize(&p, Some(&r), u, Action::Restart, 2000),
            Err(Denied::Expired)
        );
        assert_eq!(
            authorize(&p, Some(&r), u, Action::Stop, 2000),
            Err(Denied::Expired)
        );
    }

    #[test]
    fn restart_honours_the_cooldown_to_the_second() {
        let p = Policy::default();
        let r = running_at(100);
        let u = Usage::default();
        assert_eq!(
            authorize(&p, Some(&r), u, Action::Restart, 110),
            Err(Denied::Cooldown {
                retry_after_secs: 20
            })
        );
        assert_eq!(authorize(&p, Some(&r), u, Action::Restart, 130), Ok(()));
    }

    #[test]
    fn restart_spends_the_start_budget_but_ignores_concurrency() {
        let p = Policy::default();
        let r = running_at(0);
        let elsewhere = Usage {
            starts_used: 1,
            live_elsewhere: 1,
        };
        assert_eq!(
            authorize(&p, Some(&r), elsewhere, Action::Restart, 100),
            Ok(())
        );
        let spent = Usage {
            starts_used: 5,
            live_elsewhere: 0,
        };
        assert_eq!(
            authorize(&p, Some(&r), spent, Action::Restart, 100),
            Err(Denied::StartLimit { max: 5 })
        );
    }

    #[test]
    fn stop_and_restart_need_a_running_instance() {
        let p = Policy::default();
        let u = Usage::default();
        assert_eq!(
            authorize(&p, None, u, Action::Stop, 0),
            Err(Denied::NotRunning)
        );
        let stopped = InstanceRecord {
            state: InstanceState::Stopped,
            ..running_at(0)
        };
        assert_eq!(
            authorize(&p, Some(&stopped), u, Action::Restart, 99),
            Err(Denied::NotRunning)
        );
        assert_eq!(
            authorize(&p, Some(&running_at(0)), u, Action::Stop, 1),
            Ok(())
        );
    }

    #[test]
    fn reaping_waits_for_expiry_plus_grace_and_skips_finished_records() {
        let live = running_at(0);
        let stopped = InstanceRecord {
            state: InstanceState::Stopped,
            ..live
        };
        let list = [(1u32, live), (2, stopped)];
        assert!(due_for_reaping(&list, 1004, 5).is_empty());
        assert_eq!(due_for_reaping(&list, 1005, 5), vec![1]);
    }

    #[test]
    fn state_names_round_trip() {
        for s in [
            InstanceState::Starting,
            InstanceState::Running,
            InstanceState::Stopped,
            InstanceState::Expired,
            InstanceState::Failed,
        ] {
            assert_eq!(InstanceState::parse(s.as_str()), Some(s));
        }
        assert_eq!(InstanceState::parse("x"), None);
    }
}
