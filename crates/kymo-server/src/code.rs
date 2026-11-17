//! Coding challenges. The author writes the tests and picks the languages (images a lab host
//! declares); a player sends source code; a lab host runs it in a disposable container and says
//! which tests passed. All the policy is here: who may run, how often, what a player may see of
//! the tests, and when a run counts as a solve.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use kymo_lab::judge::{CaseVerdict, Compare, JudgeCase, JudgeReport, JudgeRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::error::ApiError;
use crate::jeopardy_store::Scorer;
use crate::state::AppState;

const MAX_LANGUAGES: usize = 4;
const MAX_CASES: usize = 50;
const MAX_SAMPLES: usize = 5;
const MAX_CASE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    /// Short name players pick, for example `python`.
    pub id: String,
    pub label: String,
    /// The lab host manifest's artifact for this language.
    pub artifact: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub input: String,
    pub expected: String,
    /// Shown to players with its input, expected and actual output. The others stay hidden.
    #[serde(default)]
    pub sample: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeConfig {
    pub languages: Vec<Language>,
    pub cases: Vec<Case>,
    pub time_limit_ms: u32,
    pub memory_mb: u32,
    #[serde(default)]
    pub compare: Compare,
}

fn bad(message: &str) -> ApiError {
    ApiError::BadRequest(message.into())
}

impl CodeConfig {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.languages.is_empty() || self.languages.len() > MAX_LANGUAGES {
            return Err(bad("a coding challenge has 1 to 4 languages"));
        }
        let mut seen = HashSet::new();
        for l in &self.languages {
            let id_ok = !l.id.is_empty()
                && l.id.len() <= 16
                && l.id.chars().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '+')
                });
            if !id_ok || !seen.insert(l.id.as_str()) {
                return Err(bad(
                    "a language id is 1 to 16 lowercase letters, digits, _ - +, and not repeated",
                ));
            }
            if l.label.trim().is_empty()
                || l.label.len() > 40
                || l.artifact.trim().is_empty()
                || l.artifact.len() > 200
            {
                return Err(bad(
                    "a language needs a label (up to 40 characters) and a lab host artifact",
                ));
            }
        }
        if self.cases.is_empty() || self.cases.len() > MAX_CASES {
            return Err(bad("a coding challenge has 1 to 50 tests"));
        }
        if self
            .cases
            .iter()
            .any(|c| c.input.len() > MAX_CASE_BYTES || c.expected.len() > MAX_CASE_BYTES)
        {
            return Err(bad("a test input or output is at most 256 KB"));
        }
        if self.cases.iter().filter(|c| c.sample).count() > MAX_SAMPLES {
            return Err(bad("at most 5 tests can be shown to players"));
        }
        if !(100..=10_000).contains(&self.time_limit_ms) {
            return Err(bad("the time limit is 100 ms to 10 s"));
        }
        if !(16..=1024).contains(&self.memory_mb) {
            return Err(bad("the memory limit is 16 MB to 1 GB"));
        }
        Ok(())
    }

    /// What a player is told before they submit: languages, the sample tests, the limits.
    pub fn for_player(&self) -> Value {
        json!({
            "languages": self.languages.iter().map(|l| json!({ "id": l.id, "label": l.label })).collect::<Vec<_>>(),
            "samples": self.cases.iter().filter(|c| c.sample).map(|c| json!({ "input": c.input, "expected": c.expected })).collect::<Vec<_>>(),
            "tests": self.cases.len(),
            "time_limit_ms": self.time_limit_ms,
            "memory_mb": self.memory_mb,
        })
    }

    fn request(&self, language: &Language, source: &str) -> JudgeRequest {
        JudgeRequest {
            artifact: language.artifact.clone(),
            source: source.to_owned(),
            cases: self
                .cases
                .iter()
                .map(|c| JudgeCase {
                    input: c.input.clone(),
                    expected: c.expected.clone(),
                })
                .collect(),
            time_limit_ms: self.time_limit_ms,
            memory_mb: self.memory_mb,
            compare: self.compare,
            stop_on_fail: false,
        }
    }

    pub fn language(&self, id: &str) -> Result<&Language, ApiError> {
        self.languages
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| bad("this challenge does not accept that language"))
    }
}

pub async fn config_of(
    pool: &SqlitePool,
    challenge_id: i64,
) -> Result<Option<CodeConfig>, ApiError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT config FROM challenge_code WHERE challenge_id = ?")
            .bind(challenge_id)
            .fetch_optional(pool)
            .await?;
    Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
}

pub async fn configs_of_event(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<HashMap<i64, CodeConfig>, ApiError> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT c.challenge_id, c.config FROM challenge_code c JOIN challenge ch ON ch.id = c.challenge_id WHERE ch.event_id = ?",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, raw)| serde_json::from_str(&raw).ok().map(|c| (id, c)))
        .collect())
}

/// Marks a challenge as a coding challenge before it has any tests. Until it has them it cannot
/// be published and a run is refused.
pub async fn mark(pool: &SqlitePool, challenge_id: i64) -> Result<(), ApiError> {
    sqlx::query("INSERT OR IGNORE INTO challenge_code (challenge_id, config) VALUES (?, '{}')")
        .bind(challenge_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn put_config(
    pool: &SqlitePool,
    challenge_id: i64,
    config: &CodeConfig,
) -> Result<(), ApiError> {
    let raw = serde_json::to_string(config).map_err(|_| ApiError::Internal)?;
    sqlx::query("INSERT INTO challenge_code (challenge_id, config) VALUES (?, ?) ON CONFLICT(challenge_id) DO UPDATE SET config = excluded.config")
        .bind(challenge_id)
        .bind(raw)
        .execute(pool)
        .await?;
    Ok(())
}

/// Coding runs in progress, as (challenge, participant). It lives in the app state, so two apps in
/// one process (tests) never see each other's runs.
pub type Runs = std::sync::Arc<Mutex<HashSet<(i64, i64)>>>;

/// One run at a time per participant and challenge: a second click while the first still runs is
/// told so, instead of starting more containers.
pub struct RunGuard {
    runs: Runs,
    key: (i64, i64),
}

impl RunGuard {
    pub fn take(runs: &Runs, challenge_id: i64, scorer: Scorer) -> Result<Self, ApiError> {
        let key = (
            challenge_id,
            scorer.id() * 2 + i64::from(matches!(scorer, Scorer::Team(_))),
        );
        let mut set = runs.lock().unwrap_or_else(|e| e.into_inner());
        if !set.insert(key) {
            return Err(ApiError::Conflict("your last run is still going".into()));
        }
        Ok(Self {
            runs: std::sync::Arc::clone(runs),
            key,
        })
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}

/// Two accepted solutions this alike are reported to the organizers.
const COPY_THRESHOLD: f64 = 0.85;
/// Other solutions looked at per new one: the most recent, so the work stays bounded.
const COMPARED: i64 = 500;

async fn participant_name(pool: &SqlitePool, kind: &str, id: i64) -> Result<String, ApiError> {
    let sql = if kind == "team" {
        "SELECT name FROM player_team WHERE id = ?"
    } else {
        "SELECT COALESCE(NULLIF(display_name, ''), username) FROM user WHERE id = ?"
    };
    Ok(sqlx::query_scalar::<_, String>(sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .unwrap_or_else(|| format!("{kind} {id}")))
}

/// Compares a newly accepted solution with the other accepted ones of the challenge and records
/// every pair that looks copied. A failure here never fails the player's run.
pub async fn note_matches(
    state: &AppState,
    event_id: i64,
    challenge_id: i64,
    scorer: Scorer,
    submission_id: i64,
    source: &str,
) -> Result<usize, ApiError> {
    let mine = kymo_jeopardy::similarity::fingerprint(source);
    if !kymo_jeopardy::similarity::comparable(&mine) {
        return Ok(0);
    }
    let others: Vec<(i64, String, i64, String)> = sqlx::query_as(
        "SELECT id, scorer_kind, scorer_id, source FROM code_submission WHERE challenge_id = ? AND passed = total AND NOT (scorer_kind = ? AND scorer_id = ?) ORDER BY id DESC LIMIT ?",
    )
    .bind(challenge_id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .bind(COMPARED)
    .fetch_all(&state.pool)
    .await?;
    let name = participant_name(&state.pool, scorer.kind(), scorer.id()).await?;
    let mut seen = HashSet::new();
    let mut found = 0;
    for (other_id, kind, who, other_source) in others {
        // One look per participant: their latest accepted solution.
        if !seen.insert((kind.clone(), who)) {
            continue;
        }
        let score = kymo_jeopardy::similarity::similarity(
            &mine,
            &kymo_jeopardy::similarity::fingerprint(&other_source),
        );
        if score < COPY_THRESHOLD {
            continue;
        }
        let other_name = participant_name(&state.pool, &kind, who).await?;
        sqlx::query("INSERT OR IGNORE INTO code_match (event_id, challenge_id, submission_id, other_submission_id, name, other_name, score, at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(event_id)
            .bind(challenge_id)
            .bind(submission_id)
            .bind(other_id)
            .bind(&name)
            .bind(&other_name)
            .bind((score * 100.0).round() as i64)
            .bind(state.now())
            .execute(&state.pool)
            .await?;
        found += 1;
    }
    if found > 0 {
        crate::monitor_store::audit(
            &state.pool,
            state.now(),
            None,
            "code_similarity",
            "challenge",
            challenge_id,
            &format!("{name}: {found} similar solution(s)"),
        )
        .await?;
    }
    Ok(found)
}

/// Runs the source on the lab host and returns what it said. Nothing is recorded here.
pub async fn judge(
    state: &AppState,
    lab_host_id: Option<i64>,
    config: &CodeConfig,
    language: &str,
    source: &str,
) -> Result<JudgeReport, ApiError> {
    let language = config.language(language)?;
    let Some(host) = lab_host_id else {
        return Err(ApiError::Unavailable(
            "coding challenges run on a lab host: the event has none".into(),
        ));
    };
    let row = crate::lab_store::get(&state.pool, host).await?;
    if !row.enabled {
        return Err(ApiError::Unavailable(
            "the lab host for this challenge is switched off".into(),
        ));
    }
    let client = state.labs().client(&row)?;
    client
        .judge(&config.request(language, source))
        .await
        .map_err(|e| match e {
            kymo_instances::RuntimeError::Unavailable(m) => ApiError::Unavailable(m),
            other => ApiError::Unavailable(format!("the judge could not run this: {other}")),
        })
}

/// A report as JSON. A player sees every verdict and time, and the input, expected and actual
/// output only of the sample tests; an author checking their reference solution sees everything.
pub fn report_json(config: &CodeConfig, report: &JudgeReport, reveal_all: bool) -> Value {
    let cases: Vec<Value> = report
        .cases
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut out = json!({ "verdict": c.verdict, "ms": c.ms, "sample": config.cases.get(i).is_some_and(|x| x.sample) });
            if reveal_all || config.cases.get(i).is_some_and(|x| x.sample) {
                if let Some(case) = config.cases.get(i) {
                    out["input"] = json!(case.input);
                    out["expected"] = json!(case.expected);
                }
                out["stdout"] = json!(c.stdout);
                out["stderr"] = json!(c.stderr);
            }
            out
        })
        .collect();
    json!({
        "compile_error": report.compile_error,
        "passed": report.passed(),
        "total": config.cases.len(),
        "cases": cases,
    })
}

/// Whether the run solves the challenge: it compiled and every test was accepted.
pub fn is_solution(report: &JudgeReport, total: usize) -> bool {
    report.compile_error.is_none()
        && report.cases.len() == total
        && report
            .cases
            .iter()
            .all(|c| c.verdict == CaseVerdict::Accepted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kymo_lab::judge::CaseReport;

    fn config() -> CodeConfig {
        CodeConfig {
            languages: vec![Language {
                id: "python".into(),
                label: "Python 3".into(),
                artifact: "judge/py:1".into(),
            }],
            cases: vec![
                Case {
                    input: "1 2".into(),
                    expected: "3".into(),
                    sample: true,
                },
                Case {
                    input: "secret".into(),
                    expected: "hidden answer".into(),
                    sample: false,
                },
            ],
            time_limit_ms: 1000,
            memory_mb: 64,
            compare: Compare::Lines,
        }
    }

    fn report(verdicts: &[CaseVerdict]) -> JudgeReport {
        JudgeReport {
            compile_error: None,
            cases: verdicts
                .iter()
                .map(|v| CaseReport {
                    verdict: *v,
                    ms: 5,
                    exit_code: Some(0),
                    stdout: "out".into(),
                    stderr: "err".into(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_config_is_checked() {
        assert!(config().validate().is_ok());
        let mut c = config();
        c.languages.push(c.languages[0].clone());
        assert!(c.validate().is_err(), "a repeated id");
        let mut c = config();
        c.languages[0].id = "Py Thon".into();
        assert!(c.validate().is_err());
        let mut c = config();
        c.cases.clear();
        assert!(c.validate().is_err());
        let mut c = config();
        c.cases = (0..6)
            .map(|_| Case {
                input: String::new(),
                expected: String::new(),
                sample: true,
            })
            .collect();
        assert!(c.validate().is_err(), "too many shown");
        let mut c = config();
        c.time_limit_ms = 50;
        assert!(c.validate().is_err());
        let mut c = config();
        c.memory_mb = 4096;
        assert!(c.validate().is_err());
    }

    #[test]
    fn a_player_never_gets_the_hidden_tests() {
        let c = config();
        let told = c.for_player().to_string();
        assert!(
            told.contains("\"1 2\"") && !told.contains("secret") && !told.contains("hidden answer")
        );
        let r = report(&[CaseVerdict::Accepted, CaseVerdict::WrongAnswer]);
        let shown = report_json(&c, &r, false);
        assert_eq!(shown["passed"], 1);
        assert_eq!(shown["cases"][0]["input"], "1 2");
        assert!(
            shown["cases"][1].get("input").is_none()
                && shown["cases"][1].get("expected").is_none()
                && shown["cases"][1].get("stdout").is_none()
        );
        assert_eq!(shown["cases"][1]["verdict"], "wrong_answer");
        // An author sees all of it.
        let all = report_json(&c, &r, true);
        assert_eq!(all["cases"][1]["expected"], "hidden answer");
    }

    #[test]
    fn only_a_clean_full_pass_is_a_solution() {
        assert!(is_solution(
            &report(&[CaseVerdict::Accepted, CaseVerdict::Accepted]),
            2
        ));
        assert!(!is_solution(
            &report(&[CaseVerdict::Accepted, CaseVerdict::WrongAnswer]),
            2
        ));
        assert!(!is_solution(
            &report(&[CaseVerdict::Accepted, CaseVerdict::Skipped]),
            2
        ));
        assert!(
            !is_solution(&report(&[CaseVerdict::Accepted]), 2),
            "a missing test is not a pass"
        );
        let mut r = report(&[]);
        r.compile_error = Some("boom".into());
        assert!(!is_solution(&r, 0));
    }

    #[test]
    fn a_second_run_at_once_is_refused_and_the_slot_frees_up() {
        let runs = Runs::default();
        let scorer = Scorer::User(7);
        let first = RunGuard::take(&runs, 900, scorer).unwrap();
        assert!(RunGuard::take(&runs, 900, scorer).is_err());
        assert!(
            RunGuard::take(&runs, 900, Scorer::Team(7)).is_ok(),
            "a team is not the same participant as a user"
        );
        assert!(
            RunGuard::take(&runs, 901, scorer).is_ok(),
            "another challenge is free"
        );
        assert!(
            RunGuard::take(&Runs::default(), 900, scorer).is_ok(),
            "another app does not see it"
        );
        drop(first);
        assert!(RunGuard::take(&runs, 900, scorer).is_ok());
    }
}
