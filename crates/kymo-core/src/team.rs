use crate::event::{EventState, Mode};

/// Roster rules of an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamRules {
    pub min_size: u32,
    pub max_size: u32,
    pub allow_invites: bool,
    pub roster_locked_after_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeamError {
    #[error("team is full")]
    Full,
    #[error("team is empty")]
    Empty,
    #[error("invites are disabled")]
    InvitesDisabled,
    #[error("roster is locked")]
    RosterLocked,
    #[error("event is closed")]
    EventClosed,
}

impl TeamRules {
    /// Rules for a solo event: one member, no invites.
    pub fn solo(roster_locked_after_start: bool) -> Self {
        TeamRules {
            min_size: 1,
            max_size: 1,
            allow_invites: false,
            roster_locked_after_start,
        }
    }

    /// Apply the mode: solo forces size 1 and no invites.
    pub fn for_mode(mode: Mode, configured: TeamRules) -> Self {
        match mode {
            Mode::Solo => TeamRules::solo(configured.roster_locked_after_start),
            Mode::Team => configured,
        }
    }

    /// True when `size` satisfies the minimum and maximum.
    pub fn is_valid_size(&self, size: u32) -> bool {
        size >= self.min_size && size <= self.max_size
    }
}

fn check_state(rules: &TeamRules, state: EventState) -> Result<(), TeamError> {
    match state {
        EventState::Ended | EventState::Archived => Err(TeamError::EventClosed),
        EventState::Live | EventState::Paused if rules.roster_locked_after_start => {
            Err(TeamError::RosterLocked)
        }
        _ => Ok(()),
    }
}

pub fn can_join(rules: &TeamRules, current_size: u32, state: EventState) -> Result<(), TeamError> {
    check_state(rules, state)?;
    if current_size >= rules.max_size {
        return Err(TeamError::Full);
    }
    Ok(())
}

pub fn can_leave(rules: &TeamRules, current_size: u32, state: EventState) -> Result<(), TeamError> {
    check_state(rules, state)?;
    if current_size == 0 {
        return Err(TeamError::Empty);
    }
    Ok(())
}

pub fn can_invite(
    rules: &TeamRules,
    current_size: u32,
    state: EventState,
) -> Result<(), TeamError> {
    if !rules.allow_invites {
        return Err(TeamError::InvitesDisabled);
    }
    can_join(rules, current_size, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use EventState::*;

    fn rules() -> TeamRules {
        TeamRules {
            min_size: 2,
            max_size: 4,
            allow_invites: true,
            roster_locked_after_start: true,
        }
    }

    #[test]
    fn join_size_edges() {
        let r = rules();
        assert_eq!(can_join(&r, 0, RegistrationOpen), Ok(()));
        assert_eq!(can_join(&r, 3, RegistrationOpen), Ok(()));
        assert_eq!(can_join(&r, 4, RegistrationOpen), Err(TeamError::Full));
        assert_eq!(can_join(&r, 9, RegistrationOpen), Err(TeamError::Full));
    }

    #[test]
    fn join_by_state() {
        let r = rules();
        assert_eq!(can_join(&r, 1, Draft), Ok(()));
        assert_eq!(can_join(&r, 1, Live), Err(TeamError::RosterLocked));
        assert_eq!(can_join(&r, 1, Paused), Err(TeamError::RosterLocked));
        assert_eq!(can_join(&r, 1, Ended), Err(TeamError::EventClosed));
        assert_eq!(can_join(&r, 1, Archived), Err(TeamError::EventClosed));
    }

    #[test]
    fn unlocked_roster_allows_live_changes() {
        let r = TeamRules {
            roster_locked_after_start: false,
            ..rules()
        };
        assert_eq!(can_join(&r, 1, Live), Ok(()));
        assert_eq!(can_join(&r, 1, Paused), Ok(()));
        assert_eq!(can_leave(&r, 2, Live), Ok(()));
        assert_eq!(can_join(&r, 1, Ended), Err(TeamError::EventClosed));
    }

    #[test]
    fn leave_rules() {
        let r = rules();
        assert_eq!(can_leave(&r, 1, RegistrationOpen), Ok(()));
        assert_eq!(can_leave(&r, 0, RegistrationOpen), Err(TeamError::Empty));
        assert_eq!(can_leave(&r, 3, Live), Err(TeamError::RosterLocked));
        assert_eq!(can_leave(&r, 3, Ended), Err(TeamError::EventClosed));
    }

    #[test]
    fn invite_rules() {
        let r = rules();
        assert_eq!(can_invite(&r, 3, RegistrationOpen), Ok(()));
        assert_eq!(can_invite(&r, 4, RegistrationOpen), Err(TeamError::Full));
        assert_eq!(can_invite(&r, 1, Live), Err(TeamError::RosterLocked));
        let off = TeamRules {
            allow_invites: false,
            ..r
        };
        assert_eq!(
            can_invite(&off, 1, RegistrationOpen),
            Err(TeamError::InvitesDisabled)
        );
    }

    #[test]
    fn solo_mode_forces_one_and_no_invites() {
        let r = TeamRules::for_mode(Mode::Solo, rules());
        assert_eq!((r.min_size, r.max_size, r.allow_invites), (1, 1, false));
        assert!(r.roster_locked_after_start);
        assert_eq!(can_join(&r, 0, RegistrationOpen), Ok(()));
        assert_eq!(can_join(&r, 1, RegistrationOpen), Err(TeamError::Full));
        assert_eq!(
            can_invite(&r, 0, RegistrationOpen),
            Err(TeamError::InvitesDisabled)
        );
    }

    #[test]
    fn team_mode_keeps_configured_rules() {
        assert_eq!(TeamRules::for_mode(Mode::Team, rules()), rules());
    }

    #[test]
    fn valid_size_bounds() {
        let r = rules();
        assert!(!r.is_valid_size(1));
        assert!(r.is_valid_size(2));
        assert!(r.is_valid_size(4));
        assert!(!r.is_valid_size(5));
    }

    #[test]
    fn zero_max_never_joinable() {
        let r = TeamRules {
            max_size: 0,
            ..rules()
        };
        assert_eq!(can_join(&r, 0, Draft), Err(TeamError::Full));
    }
}
