use thiserror::Error;

use crate::model::MatchId;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error("at least {min} teams are required")]
    TooFewTeams { min: usize },
    #[error("at most {max} teams are supported")]
    TooManyTeams { max: usize },
    #[error("team {0} appears more than once")]
    DuplicateTeam(u32),
    #[error("team {0} is not in this bracket")]
    UnknownTeam(u32),
    #[error("match {0} does not exist")]
    UnknownMatch(MatchId),
    #[error("match {0} is waiting for its teams")]
    NotReady(MatchId),
    #[error("scores cannot be tied in a knockout match")]
    Tie,
    #[error("match {0} has no recorded result")]
    NoResult(MatchId),
    #[error("{0}")]
    BadGroupSetup(String),
    #[error("a best of {0} series needs a majority of games, with no tied game")]
    BadSeries(u8),
    #[error("a later match depends on this result and was already played")]
    DownstreamPlayed,
}
