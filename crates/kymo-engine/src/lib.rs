mod error;
mod generate;
mod model;
mod play;

pub use error::EngineError;
pub use generate::{MAX_TEAMS, generate};
pub use model::{Bracket, Dest, Format, Match, MatchId, Side, Slot, Stage, TeamId};
