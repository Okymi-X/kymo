//! The platform's copy of every Responder Verifier run. The Verifier container and its log are
//! removed with the instance, so a run is copied here while the instance lives and once more as
//! it ends; organizers read it afterwards. Rules for the copy only: what a run means lives in the
//! Verifier and the lab host.

use std::collections::HashMap;

use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::error::ApiError;

/// Check output comes from a machine the player controls, so it is cut to a sensible size.
const ID_MAX: usize = 64;
const DETAIL_MAX: usize = 500;
const CHECKS_MAX: usize = 64;

/// One run of a Verifier as the lab host reported it, reduced to what the platform keeps.
#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub at: i64,
    pub millis: i64,
    pub passed: i64,
    pub total: i64,
    pub checks: Vec<Value>,
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Reads the lab host's log (an array of `{at, millis, checks: [{id, state, detail}]}`). Entries
/// that do not have that shape are skipped, and unknown states read as `broken`.
pub fn parse_log(log: &Value) -> Vec<Run> {
    let Some(entries) = log.as_array() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let at = entry.get("at")?.as_i64()?;
            let millis = entry
                .get("millis")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0);
            let checks: Vec<Value> = entry
                .get("checks")?
                .as_array()?
                .iter()
                .take(CHECKS_MAX)
                .filter_map(|check| {
                    let id = check.get("id")?.as_str()?;
                    let state = check
                        .get("state")
                        .and_then(Value::as_str)
                        .filter(|s| matches!(*s, "fixed" | "failed"))
                        .unwrap_or("broken");
                    let detail = check.get("detail").and_then(Value::as_str).unwrap_or("");
                    Some(json!({
                        "id": clip(id, ID_MAX),
                        "state": state,
                        "detail": clip(detail, DETAIL_MAX),
                    }))
                })
                .collect();
            let passed = checks.iter().filter(|c| c["state"] == "fixed").count() as i64;
            Some(Run {
                at,
                millis,
                passed,
                total: checks.len() as i64,
                checks,
            })
        })
        .collect()
}

/// Copies the runs of one instance. Runs already stored are left alone, so this is safe to call
/// as often as wanted. Returns how many were new.
pub async fn archive(pool: &SqlitePool, instance_id: i64, log: &Value) -> Result<u64, ApiError> {
    let mut added = 0;
    for run in parse_log(log) {
        let checks = serde_json::to_string(&run.checks).map_err(|_| ApiError::Internal)?;
        let done = sqlx::query(
            "INSERT OR IGNORE INTO verify_run (instance_id, at, millis, passed, total, checks) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(instance_id)
        .bind(run.at)
        .bind(run.millis)
        .bind(run.passed)
        .bind(run.total)
        .bind(checks)
        .execute(pool)
        .await?;
        added += done.rows_affected();
    }
    Ok(added)
}

/// The stored runs of an instance, oldest first, in the shape the lab host's log has.
pub async fn list(pool: &SqlitePool, instance_id: i64) -> Result<Vec<Value>, ApiError> {
    let rows: Vec<(i64, i64, i64, i64, String)> = sqlx::query_as(
        "SELECT at, millis, passed, total, checks FROM verify_run WHERE instance_id = ? ORDER BY at, id",
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(at, millis, passed, total, checks)| {
            json!({
                "at": at,
                "millis": millis,
                "passed": passed,
                "total": total,
                "checks": serde_json::from_str::<Value>(&checks).unwrap_or(Value::Array(Vec::new())),
            })
        })
        .collect())
}

/// How far one instance got, for the organizer's list: how many times Verify was pressed, the
/// latest result, and when every check was first green.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub runs: i64,
    pub last_at: i64,
    pub last_passed: i64,
    pub last_total: i64,
    pub green_at: Option<i64>,
}

impl Summary {
    pub fn to_json(&self) -> Value {
        json!({
            "runs": self.runs,
            "last_at": self.last_at,
            "passed": self.last_passed,
            "total": self.last_total,
            "green_at": self.green_at,
        })
    }
}

/// A summary for every instance of an event that has at least one stored run.
pub async fn summaries(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<HashMap<i64, Summary>, ApiError> {
    let rows: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT v.instance_id, v.at, v.passed, v.total FROM verify_run v JOIN instance i ON i.id = v.instance_id WHERE i.event_id = ? ORDER BY v.instance_id, v.at, v.id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<i64, Summary> = HashMap::new();
    for (instance_id, at, passed, total) in rows {
        let s = out.entry(instance_id).or_default();
        s.runs += 1;
        s.last_at = at;
        s.last_passed = passed;
        s.last_total = total;
        if s.green_at.is_none() && total > 0 && passed == total {
            s.green_at = Some(at);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_is_reduced_to_what_the_platform_keeps() {
        let log = json!([
            { "at": 100, "millis": 9000, "checks": [
                { "id": "webhook", "state": "fixed", "detail": "" },
                { "id": "secret", "state": "failed", "detail": "the leaked partner key is still accepted" },
            ]},
            { "at": 200, "millis": 8000, "checks": [
                { "id": "webhook", "state": "fixed", "detail": "" },
                { "id": "secret", "state": "mystery", "detail": "x" },
            ]},
        ]);
        let runs = parse_log(&log);
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].passed, runs[0].total), (1, 2));
        assert_eq!(
            runs[1].checks[1]["state"], "broken",
            "an unknown state is not trusted"
        );
    }

    #[test]
    fn malformed_entries_are_skipped_and_text_is_cut() {
        let long = "x".repeat(2000);
        let log = json!([
            "not an entry",
            { "millis": 1, "checks": [] },
            { "at": 5, "checks": "no" },
            { "at": 6, "millis": -4, "checks": [{ "id": long, "state": "fixed", "detail": long }, { "state": "fixed" }] },
        ]);
        let runs = parse_log(&log);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].millis, 0, "a negative duration is not kept");
        assert_eq!(runs[0].total, 1, "a check with no id is dropped");
        assert_eq!(runs[0].checks[0]["id"].as_str().map(str::len), Some(ID_MAX));
        assert_eq!(
            runs[0].checks[0]["detail"].as_str().map(str::len),
            Some(DETAIL_MAX)
        );
        assert!(parse_log(&json!({ "not": "an array" })).is_empty());
    }
}
