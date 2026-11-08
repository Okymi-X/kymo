use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Stored lifecycle state of an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventState {
    Draft,
    RegistrationOpen,
    Live,
    Paused,
    Ended,
    Archived,
}

impl EventState {
    pub const ALL: [EventState; 6] = [
        EventState::Draft,
        EventState::RegistrationOpen,
        EventState::Live,
        EventState::Paused,
        EventState::Ended,
        EventState::Archived,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EventState::Draft => "draft",
            EventState::RegistrationOpen => "registration_open",
            EventState::Live => "live",
            EventState::Paused => "paused",
            EventState::Ended => "ended",
            EventState::Archived => "archived",
        }
    }

    /// True when a manual move from `self` to `next` is allowed.
    pub fn can_move_to(self, next: EventState) -> Result<(), CoreError> {
        use EventState::*;
        let ok = matches!(
            (self, next),
            (Draft, RegistrationOpen)
                | (Draft, Live)
                | (RegistrationOpen, Draft)
                | (RegistrationOpen, Live)
                | (Live, Paused)
                | (Live, Ended)
                | (Paused, Live)
                | (Paused, Ended)
                | (Ended, Live)
                | (Ended, Archived)
        );
        if ok {
            Ok(())
        } else {
            Err(CoreError::InvalidTransition {
                from: self,
                to: next,
            })
        }
    }
}

impl FromStr for EventState {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        EventState::ALL
            .into_iter()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| CoreError::Unknown {
                kind: "event state",
                value: s.to_string(),
            })
    }
}

/// What kind of competition an event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Tournament,
    Jeopardy,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Tournament => "tournament",
            EventKind::Jeopardy => "jeopardy",
        }
    }
}

impl FromStr for EventKind {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tournament" => Ok(EventKind::Tournament),
            "jeopardy" => Ok(EventKind::Jeopardy),
            _ => Err(CoreError::Unknown {
                kind: "event kind",
                value: s.to_string(),
            }),
        }
    }
}

/// Whether participants play alone or in teams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Solo,
    Team,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Solo => "solo",
            Mode::Team => "team",
        }
    }
}

impl FromStr for Mode {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "solo" => Ok(Mode::Solo),
            "team" => Ok(Mode::Team),
            _ => Err(CoreError::Unknown {
                kind: "mode",
                value: s.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EventState::*;

    const ALLOWED: [(EventState, EventState); 10] = [
        (Draft, RegistrationOpen),
        (Draft, Live),
        (RegistrationOpen, Draft),
        (RegistrationOpen, Live),
        (Live, Paused),
        (Live, Ended),
        (Paused, Live),
        (Paused, Ended),
        (Ended, Live),
        (Ended, Archived),
    ];

    #[test]
    fn transition_table_is_exact() {
        for from in EventState::ALL {
            for to in EventState::ALL {
                let expected = ALLOWED.contains(&(from, to));
                assert_eq!(from.can_move_to(to).is_ok(), expected, "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn invalid_transition_carries_states() {
        assert_eq!(
            Archived.can_move_to(Live),
            Err(CoreError::InvalidTransition {
                from: Archived,
                to: Live
            })
        );
    }

    #[test]
    fn archived_is_terminal() {
        for to in EventState::ALL {
            assert!(Archived.can_move_to(to).is_err());
        }
    }

    #[test]
    fn state_str_roundtrip() {
        for s in EventState::ALL {
            assert_eq!(s.as_str().parse::<EventState>(), Ok(s));
        }
        assert!("nope".parse::<EventState>().is_err());
    }

    #[test]
    fn state_serde_snake_case() {
        let json = serde_json::to_string(&RegistrationOpen).unwrap();
        assert_eq!(json, "\"registration_open\"");
        let back: EventState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, RegistrationOpen);
    }

    #[test]
    fn kind_and_mode_roundtrip() {
        for k in [EventKind::Tournament, EventKind::Jeopardy] {
            assert_eq!(k.as_str().parse::<EventKind>(), Ok(k));
        }
        for m in [Mode::Solo, Mode::Team] {
            assert_eq!(m.as_str().parse::<Mode>(), Ok(m));
        }
        assert!("x".parse::<EventKind>().is_err());
        assert!("x".parse::<Mode>().is_err());
        assert_eq!(serde_json::to_string(&Mode::Team).unwrap(), "\"team\"");
        assert_eq!(
            serde_json::to_string(&EventKind::Jeopardy).unwrap(),
            "\"jeopardy\""
        );
    }
}
