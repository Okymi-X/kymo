//! Pure Jeopardy (CTF) engine: flag matching, scoring, ranking and
//! submission rate math. No IO, no async, no clock, no randomness; time is
//! passed in as unix seconds.

pub mod board;
pub mod dynamic;
pub mod error;
pub mod flag;
pub mod limit;
pub mod scoring;

pub use board::{
    BoardConfig, Challenge, DuplicatePolicy, MemberBreakdown, MemberSolve, Participant, Row, Solve,
    Step, TieBreak, first_blood, progression, rank, solve_counts, team_breakdown,
};
pub use error::{FlagError, ScoringError};
pub use flag::{FlagFormat, FlagSpec, Verdict, check, ct_eq};
pub use limit::{AttemptCap, AttemptCapReached, RateDenied, RateRule, RateState};
pub use scoring::{Curve, Hint, Scoring, current_value, points_for, score_for_solve};
