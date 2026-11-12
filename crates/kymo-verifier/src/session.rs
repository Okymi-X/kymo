//! The state of one Verifier: which checks passed in the latest run, when the next run may
//! start, and when the flag may be shown. Pure: time is passed in.

use std::sync::Arc;

use serde::Serialize;

use crate::definition::Definition;
use crate::runner::{Outcome, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// A run is already going.
    Busy,
    /// Too soon after the last run. Seconds to wait.
    Wait(u64),
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckView {
    pub id: String,
    pub title: String,
    /// `None` until the first run.
    pub state: Option<State>,
    pub message: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub title: String,
    pub checks: Vec<CheckView>,
    pub last_run_at: Option<i64>,
    pub running: bool,
    /// Seconds until Verify may be pressed again.
    pub wait_secs: u64,
    pub expires_at: Option<i64>,
    /// Present only while the latest run had every check fixed.
    pub flag: Option<String>,
}

pub struct Session {
    definition: Arc<Definition>,
    flag: Option<String>,
    expires_at: Option<i64>,
    states: Vec<Option<State>>,
    last_run_at: Option<i64>,
    running: bool,
    /// Whether the latest run finished with every check fixed.
    clean: bool,
}

impl Session {
    pub fn new(
        definition: Arc<Definition>,
        flag: Option<String>,
        expires_at: Option<i64>,
    ) -> Session {
        let states = vec![None; definition.checks.len()];
        Session {
            definition,
            flag,
            expires_at,
            states,
            last_run_at: None,
            running: false,
            clean: false,
        }
    }

    pub fn definition(&self) -> &Definition {
        &self.definition
    }

    fn wait_secs(&self, now: i64) -> u64 {
        match self.last_run_at {
            Some(at) => {
                let ready = at.saturating_add(
                    i64::try_from(self.definition.min_interval_secs).unwrap_or(i64::MAX),
                );
                u64::try_from((ready - now).max(0)).unwrap_or(0)
            }
            None => 0,
        }
    }

    /// Starts a run if one may start now.
    pub fn begin(&mut self, now: i64) -> Result<(), Refused> {
        if self.running {
            return Err(Refused::Busy);
        }
        let wait = self.wait_secs(now);
        if wait > 0 {
            return Err(Refused::Wait(wait));
        }
        self.running = true;
        Ok(())
    }

    /// Records the results of a run that was started with `begin`.
    pub fn finish(&mut self, now: i64, outcomes: &[Outcome]) {
        self.states = self
            .definition
            .checks
            .iter()
            .enumerate()
            .map(|(i, _)| outcomes.get(i).map(|o| o.state))
            .collect();
        self.clean = !outcomes.is_empty()
            && outcomes.len() == self.definition.checks.len()
            && outcomes.iter().all(|o| o.state == State::Fixed);
        self.last_run_at = Some(now);
        self.running = false;
    }

    /// A run that could not be carried out at all: nothing is trusted from it.
    pub fn abandon(&mut self, now: i64) {
        self.states
            .iter_mut()
            .for_each(|s| *s = Some(State::Broken));
        self.clean = false;
        self.last_run_at = Some(now);
        self.running = false;
    }

    pub fn view(&self, now: i64) -> View {
        let checks = self
            .definition
            .checks
            .iter()
            .zip(&self.states)
            .map(|(check, state)| CheckView {
                id: check.id.clone(),
                title: check.title.clone(),
                state: *state,
                message: match state {
                    None => "Run Verify",
                    Some(State::Fixed) => leak(self.definition.message_fixed()),
                    Some(State::Failed) => leak(self.definition.message_failed()),
                    Some(State::Broken) => leak(self.definition.message_broken()),
                },
            })
            .collect();
        View {
            title: self.definition.title.clone().unwrap_or_default(),
            checks,
            last_run_at: self.last_run_at,
            running: self.running,
            wait_secs: self.wait_secs(now),
            expires_at: self.expires_at,
            flag: if self.clean { self.flag.clone() } else { None },
        }
    }
}

/// The three messages come from the definition, which lives as long as the process; the view
/// borrows them for its short life.
fn leak(text: &str) -> &'static str {
    // Messages are a handful of short strings per Verifier; interning them once is enough.
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(found) = map.get(text) {
        return found;
    }
    let owned: &'static str = Box::leak(text.to_owned().into_boxed_str());
    map.insert(text.to_owned(), owned);
    owned
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(flag: Option<&str>) -> Session {
        let definition: Definition = serde_json::from_str(
            r#"{"version":1,"min_interval_secs":20,"checks":[
              {"id":"a","title":"A","language":"bash","file":"c.sh","function":"a"},
              {"id":"b","title":"B","language":"bash","file":"c.sh","function":"b"}]}"#,
        )
        .unwrap();
        Session::new(Arc::new(definition), flag.map(str::to_owned), Some(5000))
    }

    fn out(states: &[State]) -> Vec<Outcome> {
        states
            .iter()
            .map(|s| Outcome {
                state: *s,
                detail: String::new(),
                millis: 0,
            })
            .collect()
    }

    #[test]
    fn nothing_has_run_and_no_flag_is_shown() {
        let s = session(Some("KYMO{x}"));
        let v = s.view(100);
        assert!(
            v.checks
                .iter()
                .all(|c| c.state.is_none() && c.message == "Run Verify")
        );
        assert_eq!(v.flag, None);
        assert_eq!(v.wait_secs, 0);
        assert_eq!(v.expires_at, Some(5000));
    }

    #[test]
    fn the_flag_appears_only_when_the_latest_run_was_all_fixed() {
        let mut s = session(Some("KYMO{x}"));
        s.begin(100).unwrap();
        s.finish(100, &out(&[State::Fixed, State::Failed]));
        assert_eq!(s.view(101).flag, None);
        s.begin(200).unwrap();
        s.finish(200, &out(&[State::Fixed, State::Fixed]));
        let v = s.view(201);
        assert_eq!(v.flag.as_deref(), Some("KYMO{x}"));
        assert_eq!(v.checks[0].message, "Step fixed");
        // A later run with a regression withdraws it.
        s.begin(300).unwrap();
        s.finish(300, &out(&[State::Fixed, State::Broken]));
        assert_eq!(s.view(301).flag, None);
        assert_eq!(
            s.view(301).checks[1].message,
            "Something this step depends on is broken. Restore it, or restart the instance"
        );
    }

    #[test]
    fn a_session_with_no_flag_never_shows_one() {
        let mut s = session(None);
        s.begin(1).unwrap();
        s.finish(1, &out(&[State::Fixed, State::Fixed]));
        assert_eq!(s.view(2).flag, None);
    }

    #[test]
    fn one_run_at_a_time_and_a_wait_between_runs() {
        let mut s = session(None);
        s.begin(100).unwrap();
        assert_eq!(s.begin(101), Err(Refused::Busy));
        s.finish(110, &out(&[State::Fixed, State::Fixed]));
        assert_eq!(s.begin(115), Err(Refused::Wait(15)));
        assert_eq!(s.view(115).wait_secs, 15);
        assert!(s.begin(130).is_ok());
    }

    #[test]
    fn an_abandoned_run_trusts_nothing() {
        let mut s = session(Some("KYMO{x}"));
        s.begin(1).unwrap();
        s.finish(1, &out(&[State::Fixed, State::Fixed]));
        s.begin(100).unwrap();
        s.abandon(100);
        let v = s.view(101);
        assert_eq!(v.flag, None);
        assert!(v.checks.iter().all(|c| c.state == Some(State::Broken)));
        assert!(!v.running);
    }

    #[test]
    fn a_short_result_list_does_not_count_as_clean() {
        let mut s = session(Some("KYMO{x}"));
        s.begin(1).unwrap();
        s.finish(1, &out(&[State::Fixed]));
        assert_eq!(s.view(2).flag, None);
    }
}
