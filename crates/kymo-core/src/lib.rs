//! Pure platform rules: event lifecycle, time windows, roles and rosters.
//!
//! No IO, no clock, no randomness. Time is always passed in as unix seconds.

pub mod error;
pub mod event;
pub mod role;
pub mod team;
pub mod window;

pub use error::CoreError;
pub use event::{EventKind, EventState, Mode};
pub use role::{MatrixError, Permission, PermissionMatrix, Role, scoreboard_visible};
pub use team::{TeamError, TeamRules, can_invite, can_join, can_leave};
pub use window::{
    Denied, EventSchedule, RegistrationPolicy, can_register, can_register_under, can_submit,
    can_use_instance, can_view_challenges, effective_state,
};
