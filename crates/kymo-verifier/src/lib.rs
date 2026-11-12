//! The Kymo Verifier: a definition written by the organizer, the runner that executes its
//! checks, and the state of one Verifier. The web service that players open is the binary.

pub mod definition;
pub mod runner;
pub mod service;
pub mod session;

pub use definition::{Check, Definition, Language, Loaded, load};
pub use runner::{Context, Outcome, State, run_all, run_check};
pub use session::{Refused, Session, View};
