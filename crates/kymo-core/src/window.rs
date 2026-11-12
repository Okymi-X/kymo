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

/// When registration stops, whatever the event state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationPolicy {
    /// Registration closes when the event starts (on schedule, or when an organizer starts it).
    #[default]
    AtStart,
    /// Registration closes at `registration_closes`, even after the event has started.
    AtTime,
    /// Registration stays open until an organizer closes it.
    Manual,
}

impl RegistrationPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            RegistrationPolicy::AtStart => "at_start",
            RegistrationPolicy::AtTime => "at_time",
            RegistrationPolicy::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::AtStart, Self::AtTime, Self::Manual]
            .into_iter()
            .find(|p| p.as_str() == s)
    }
}

/// Registration under an explicit policy. `closed_by_organizer` is the organizer's own switch
/// and ends registration under every policy. The event must not be draft, paused, ended or
/// archived; while it is only registering the opening time holds, and once it is live the
/// policy decides.
pub fn can_register_under(
    stored: EventState,
    schedule: &EventSchedule,
    policy: RegistrationPolicy,
    closed_by_organizer: bool,
    now: i64,
) -> Result<(), Denied> {
    let state = effective_state(stored, schedule, now);
    if !matches!(state, EventState::RegistrationOpen | EventState::Live) {
        return Err(closed_reason(state));
    }
    if closed_by_organizer {
        return Err(Denied::RegistrationClosed);
    }
    if state == EventState::RegistrationOpen && schedule.registration_opens.is_some_and(|o| now < o)
    {
        return Err(Denied::RegistrationNotOpen);
    }
    let timed_out = policy == RegistrationPolicy::AtTime
        && schedule.registration_closes.is_some_and(|c| now >= c);
    let started = policy == RegistrationPolicy::AtStart && state == EventState::Live;
    let at_time_unset = policy == RegistrationPolicy::AtTime
        && state == EventState::Live
        && schedule.registration_closes.is_none();
    if timed_out || started || at_time_unset {
        return Err(Denied::RegistrationClosed);
    }
    Ok(())
}

/// Registration as it worked before policies: a closing time means "at that time", none means
/// "at the start". Kept for callers that have no policy.
pub fn can_register(stored: EventState, schedule: &EventSchedule, now: i64) -> Result<(), Denied> {
    let policy = if schedule.registration_closes.is_some() {
        RegistrationPolicy::AtTime
    } else {
        RegistrationPolicy::AtStart
    };
    can_register_under(stored, schedule, policy, false, now)
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
    fn registration_policies_decide_when_it_stops() {
        use RegistrationPolicy::*;
        let s = EventSchedule {
            registration_opens: Some(10),
            registration_closes: Some(40),
            starts: Some(30),
            ends: Some(100),
        };
        let go = |state, policy, closed, now| can_register_under(state, &s, policy, closed, now);
        // Before the opening time nobody registers, whatever the policy.
        assert_eq!(
            go(RegistrationOpen, AtStart, false, 9),
            Err(Denied::RegistrationNotOpen)
        );
        // At start: closes when the schedule starts the event.
        assert_eq!(go(RegistrationOpen, AtStart, false, 29), Ok(()));
        assert_eq!(
            go(RegistrationOpen, AtStart, false, 30),
            Err(Denied::RegistrationClosed)
        );
        assert_eq!(
            go(Live, AtStart, false, 35),
            Err(Denied::RegistrationClosed)
        );
        // At a time: carries on after the start until the closing time.
        assert_eq!(go(RegistrationOpen, AtTime, false, 35), Ok(()));
        assert_eq!(go(Live, AtTime, false, 39), Ok(()));
        assert_eq!(go(Live, AtTime, false, 40), Err(Denied::RegistrationClosed));
        // Manual: open while the event runs until the organizer closes it.
        assert_eq!(go(Live, Manual, false, 90), Ok(()));
        assert_eq!(go(Live, Manual, true, 90), Err(Denied::RegistrationClosed));
        assert_eq!(
            go(RegistrationOpen, AtStart, true, 11),
            Err(Denied::RegistrationClosed)
        );
        // Paused, ended and archived events never take registrations.
        assert_eq!(go(Paused, Manual, false, 50), Err(Denied::Paused));
        assert_eq!(go(Live, Manual, false, 100), Err(Denied::Ended));
        assert_eq!(go(Archived, Manual, false, 50), Err(Denied::Archived));
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
