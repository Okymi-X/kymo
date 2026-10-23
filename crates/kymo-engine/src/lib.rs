mod error;
mod generate;
mod model;
mod play;
mod swiss;

pub use error::EngineError;
pub use generate::{MAX_ROUND_ROBIN_TEAMS, MAX_TEAMS, Options, generate, generate_with};
pub use model::{Bracket, Dest, Format, Match, MatchId, Side, Slot, Stage, Standing, TeamId};
