use crate::event::EventState;

/// Optional schedule of an event, in unix seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventSchedule {
    pub registration_opens: Option<i64>,
    pub registration_closes: Option<i64>,
    pub starts: Option<i64>,
    pub ends: Option<i64>,
}

/// Why an action is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Denied {
    #[error("not open")]
    NotOpen,
    #[error("paused")]
    Paused,
    #[error("ended")]
    Ended,
    #[error("archived")]
    Archived,
    #[error("not released")]
    NotReleased,
    #[error("retired")]
    Retired,
    #[error("registration not open")]
    RegistrationNotOpen,
    #[error("registration closed")]
    RegistrationClosed,
}

impl Denied {
    pub fn message(self) -> &'static str {
        match self {
            Denied::NotOpen => "The event has not started yet.",
            Denied::Paused => "The event is paused.",
            Denied::Ended => "The event has ended.",
            Denied::Archived => "The event is archived.",
            Denied::NotReleased => "This challenge is not released yet.",
            Denied::Retired => "This challenge has been retired.",
            Denied::RegistrationNotOpen => "Registration is not open yet.",
            Denied::RegistrationClosed => "Registration is closed.",
        }
    }
}

/// Apply schedule driven moves to a stored state.
///
/// Draft and RegistrationOpen become Live once `starts` is reached, and Live
/// becomes Ended once `ends` is reached. Paused, Ended and Archived never
/// change. A Draft or RegistrationOpen event past `ends` with a `starts` set
/// goes straight to Ended.
pub fn effective_state(stored: EventState, schedule: &EventSchedule, now: i64) -> EventState {
    let mut state = stored;
    if matches!(state, EventState::Draft | EventState::RegistrationOpen)
        && schedule.starts.is_some_and(|s| now >= s)
    {
        state = EventState::Live;
    }
    if state == EventState::Live && schedule.ends.is_some_and(|e| now >= e) {
        state = EventState::Ended;
    }
    state
}

fn closed_reason(state: EventState) -> Denied {
    match state {
        EventState::Paused => Denied::Paused,
        EventState::Ended => Denied::Ended,
        EventState::Archived => Denied::Archived,
        _ => Denied::NotOpen,
    }
}

/// Registration is allowed while the effective state is RegistrationOpen and
/// the registration window holds. A Live event accepts late registration only
/// when `registration_closes` is set and not yet reached.
pub fn can_register(stored: EventState, schedule: &EventSchedule, now: i64) -> Result<(), Denied> {
    let state = effective_state(stored, schedule, now);
    match state {
        EventState::RegistrationOpen => {
            if schedule.registration_opens.is_some_and(|o| now < o) {
                return Err(Denied::RegistrationNotOpen);
            }
            if schedule.registration_closes.is_some_and(|c| now >= c) {
                return Err(Denied::RegistrationClosed);
            }
            Ok(())
        }
        EventState::Live => {
            if schedule.registration_closes.is_some_and(|c| now < c) {
                Ok(())
            } else {
                Err(Denied::RegistrationClosed)
            }
        }
        other => Err(closed_reason(other)),
    }
}

/// Challenges are visible while Live, Paused or Ended.
pub fn can_view_challenges(effective: EventState) -> Result<(), Denied> {
    match effective {
        EventState::Live | EventState::Paused | EventState::Ended => Ok(()),
        EventState::Archived => Err(Denied::Archived),
        EventState::Draft | EventState::RegistrationOpen => Err(Denied::NotOpen),
    }
}

/// Release and retire times of a challenge. Release is inclusive, retire is
/// exclusive.
pub type ChallengeWindow = (Option<i64>, Option<i64>);

/// Submissions need a Live event and a challenge inside its window.
pub fn can_submit(
    effective: EventState,
    challenge_window: Option<ChallengeWindow>,
    now: i64,
) -> Result<(), Denied> {
    if effective != EventState::Live {
        return Err(closed_reason(effective));
    }
    if let Some((release, retire)) = challenge_window {
        if release.is_some_and(|r| now < r) {
            return Err(Denied::NotReleased);
        }
        if retire.is_some_and(|t| now >= t) {
            return Err(Denied::Retired);
        }
    }
    Ok(())
}

/// Same rule as submitting. The caller checks the instance expiry itself.
pub fn can_use_instance(
    effective: EventState,
    challenge_window: Option<ChallengeWindow>,
    now: i64,
) -> Result<(), Denied> {
    can_submit(effective, challenge_window, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use EventState::*;

    fn sched(starts: Option<i64>, ends: Option<i64>) -> EventSchedule {
        EventSchedule {
            starts,
            ends,
            ..Default::default()
        }
    }

    #[test]
    fn start_boundary() {
        let s = sched(Some(100), Some(200));
        for st in [Draft, RegistrationOpen] {
            assert_eq!(effective_state(st, &s, 99), st);
            assert_eq!(effective_state(st, &s, 100), Live);
            assert_eq!(effective_state(st, &s, 199), Live);
        }
    }

    #[test]
    fn end_boundary() {
        let s = sched(Some(100), Some(200));
        assert_eq!(effective_state(Live, &s, 199), Live);
        assert_eq!(effective_state(Live, &s, 200), Ended);
        assert_eq!(effective_state(Live, &s, 10_000), Ended);
    }

    #[test]
    fn past_everything_goes_to_ended() {
        let s = sched(Some(100), Some(200));
        assert_eq!(effective_state(RegistrationOpen, &s, 500), Ended);
        assert_eq!(effective_state(Draft, &s, 500), Ended);
    }

    #[test]
    fn no_schedule_means_no_change() {
        let s = EventSchedule::default();
        for st in EventState::ALL {
            assert_eq!(effective_state(st, &s, i64::MAX), st);
        }
    }

    #[test]
    fn draft_without_start_stays_draft_even_past_end() {
        let s = sched(None, Some(200));
        assert_eq!(effective_state(Draft, &s, 300), Draft);
    }

    #[test]
    fn live_without_end_stays_live() {
        let s = sched(Some(100), None);
        assert_eq!(effective_state(Live, &s, 9_999), Live);
    }

    #[test]
    fn paused_ended_archived_never_change() {
        let s = sched(Some(100), Some(200));
        for st in [Paused, Ended, Archived] {
            for now in [0, 100, 150, 200, 300] {
                assert_eq!(effective_state(st, &s, now), st);
            }
        }
    }

    #[test]
    fn register_in_window() {
        let s = EventSchedule {
            registration_opens: Some(10),
            registration_closes: Some(20),
            starts: Some(30),
            ends: Some(40),
        };
        assert_eq!(
            can_register(RegistrationOpen, &s, 9),
            Err(Denied::RegistrationNotOpen)
        );
        assert_eq!(can_register(RegistrationOpen, &s, 10), Ok(()));
        assert_eq!(can_register(RegistrationOpen, &s, 19), Ok(()));
        assert_eq!(
            can_register(RegistrationOpen, &s, 20),
            Err(Denied::RegistrationClosed)
        );
    }

    #[test]
    fn register_by_state() {
        let s = EventSchedule::default();
        assert_eq!(can_register(Draft, &s, 0), Err(Denied::NotOpen));
        assert_eq!(can_register(RegistrationOpen, &s, 0), Ok(()));
        assert_eq!(can_register(Live, &s, 0), Err(Denied::RegistrationClosed));
        assert_eq!(can_register(Paused, &s, 0), Err(Denied::Paused));
        assert_eq!(can_register(Ended, &s, 0), Err(Denied::Ended));
        assert_eq!(can_register(Archived, &s, 0), Err(Denied::Archived));
    }

    #[test]
    fn late_registration_while_live() {
        let s = EventSchedule {
            registration_closes: Some(50),
            starts: Some(30),
            ..Default::default()
        };
        assert_eq!(can_register(RegistrationOpen, &s, 40), Ok(()));
        assert_eq!(can_register(Live, &s, 49), Ok(()));
        assert_eq!(can_register(Live, &s, 50), Err(Denied::RegistrationClosed));
    }

    #[test]
    fn register_uses_effective_state() {
        let s = EventSchedule {
            starts: Some(30),
            ends: Some(40),
            ..Default::default()
        };
        assert_eq!(can_register(RegistrationOpen, &s, 40), Err(Denied::Ended));
    }

    #[test]
    fn view_challenges_by_state() {
        assert_eq!(can_view_challenges(Draft), Err(Denied::NotOpen));
        assert_eq!(can_view_challenges(RegistrationOpen), Err(Denied::NotOpen));
        assert_eq!(can_view_challenges(Live), Ok(()));
        assert_eq!(can_view_challenges(Paused), Ok(()));
        assert_eq!(can_view_challenges(Ended), Ok(()));
        assert_eq!(can_view_challenges(Archived), Err(Denied::Archived));
    }

    #[test]
    fn submit_requires_live() {
        assert_eq!(can_submit(Live, None, 5), Ok(()));
        assert_eq!(can_submit(Draft, None, 5), Err(Denied::NotOpen));
        assert_eq!(can_submit(RegistrationOpen, None, 5), Err(Denied::NotOpen));
        assert_eq!(can_submit(Paused, None, 5), Err(Denied::Paused));
        assert_eq!(can_submit(Ended, None, 5), Err(Denied::Ended));
        assert_eq!(can_submit(Archived, None, 5), Err(Denied::Archived));
    }

    #[test]
    fn submit_challenge_window_boundaries() {
        let w = Some((Some(100), Some(200)));
        assert_eq!(can_submit(Live, w, 99), Err(Denied::NotReleased));
        assert_eq!(can_submit(Live, w, 100), Ok(()));
        assert_eq!(can_submit(Live, w, 199), Ok(()));
        assert_eq!(can_submit(Live, w, 200), Err(Denied::Retired));
    }

    #[test]
    fn open_ended_windows() {
        assert_eq!(can_submit(Live, Some((None, None)), 0), Ok(()));
        assert_eq!(
            can_submit(Live, Some((Some(10), None)), 5),
            Err(Denied::NotReleased)
        );
        assert_eq!(can_submit(Live, Some((Some(10), None)), 1_000_000), Ok(()));
        assert_eq!(
            can_submit(Live, Some((None, Some(10))), 10),
            Err(Denied::Retired)
        );
        assert_eq!(can_submit(Live, Some((None, Some(10))), 9), Ok(()));
    }

    #[test]
    fn instance_matches_submit() {
        let w = Some((Some(100), Some(200)));
        for st in EventState::ALL {
            for now in [50, 100, 150, 200] {
                assert_eq!(can_use_instance(st, w, now), can_submit(st, w, now));
            }
        }
    }

    #[test]
    fn messages_are_plain() {
        let all = [
            Denied::NotOpen,
            Denied::Paused,
            Denied::Ended,
            Denied::Archived,
            Denied::NotReleased,
            Denied::Retired,
            Denied::RegistrationNotOpen,
            Denied::RegistrationClosed,
        ];
        for d in all {
            assert!(!d.message().is_empty());
            assert!(d.message().is_ascii());
        }
    }
}
