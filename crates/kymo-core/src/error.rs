use crate::event::EventState;

/// Errors raised by the pure core rules.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("cannot move event from {} to {}", from.as_str(), to.as_str())]
    InvalidTransition { from: EventState, to: EventState },
    #[error("unknown {kind}: {value}")]
    Unknown { kind: &'static str, value: String },
}
