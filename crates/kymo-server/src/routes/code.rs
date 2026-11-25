//! Coding challenges over HTTP: a player runs their source against the tests, an organizer sets
//! the tests up and tries a reference solution. The rules live in `code` and `submit`.

use axum::Json;
use axum::extract::{Path, State};
use kymo_jeopardy::{current_value, score_for_solve};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{Account, ClientIp, Require, needs};
use crate::challenge_store as store;
use crate::code::{self, CodeConfig};
use crate::error::ApiError;
use crate::jeopardy_store::{self, Board, NewSubmission};
use crate::state::AppState;
use crate::submit::{challenge_of, open_for_play, play_context};

/// Longest source a player may send.
const MAX_SOURCE: usize = 64 * 1024;

#[derive(Deserialize)]
pub struct RunBody {
    language: String,
    source: String,
}

fn check_source(source: &str) -> Result<(), ApiError> {
    if source.trim().is_empty() {
        return Err(ApiError::BadRequest("write some code first".into()));
    }
    if source.len() > MAX_SOURCE {
        return Err(ApiError::BadRequest("the source is at most 64 KB".into()));
    }
    Ok(())
}

/// A player runs their code. A compile error is free (it does not use an attempt); a run that
/// reached the tests does, and a run that passes every test solves the challenge.
pub async fn run(
    Account(player, _): Account<needs::SubmitFlag>,
    ClientIp(ip): ClientIp,
    State(state): State<AppState>,
    Path((slug, challenge_id)): Path<(String, i64)>,
    Json(body): Json<RunBody>,
) -> Result<Json<Value>, ApiError> {
    check_source(&body.source)?;
    let play = play_context(&state, &slug, player.id).await?;
    let challenge = challenge_of(&state, &play, challenge_id).await?;
    if challenge.kind != "code" {
        return Err(ApiError::BadRequest(
            "this challenge takes a flag, not code".into(),
        ));
    }
    let now = state.now();
    open_for_play(&state, &play, &challenge, now).await?;
    let config = code::config_of(&state.pool, challenge.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    config.language(&body.language)?;
    let scorer = play.scorer;
    let _run = code::RunGuard::take(state.code_runs(), challenge.id, scorer)?;

    // The checks that need the database go first and the lock is let go before the code runs: it
    // is one lock for every write on the platform and a run can take many seconds.
    let used = {
        let _guard = state.write_lock.lock().await;
        if jeopardy_store::is_solved(&state.pool, challenge.id, scorer.id()).await? {
            return Err(ApiError::Conflict("already solved".into()));
        }
        let settings = jeopardy_store::settings(&state.pool, play.event.id).await?;
        state
            .rate_check(play.event.id, scorer.id(), &settings.rate_rule())
            .map_err(|denied| ApiError::RateLimited {
                retry_after: denied.retry_after_secs,
            })?;
        let used = jeopardy_store::attempts_used(&state.pool, challenge.id, scorer).await?;
        challenge
            .attempt_cap()
            .check(used)
            .map_err(|capped| ApiError::Conflict(capped.to_string()))?;
        used
    };

    let report = code::judge(
        &state,
        challenge.host(play.event.lab_host_id),
        &config,
        &body.language,
        &body.source,
    )
    .await?;
    let solved = code::is_solution(&report, config.cases.len());
    let compile_failed = report.compile_error.is_some();
    let value = format!(
        "code:{}:{}/{}",
        body.language,
        report.passed(),
        config.cases.len()
    );
    // A program that did not compile never reached a test, so it does not use an attempt.
    let verdict = if solved {
        "correct"
    } else if compile_failed {
        "bad_format"
    } else {
        "incorrect"
    };

    let _guard = state.write_lock.lock().await;
    if jeopardy_store::is_solved(&state.pool, challenge.id, scorer.id()).await? {
        return Err(ApiError::Conflict("already solved".into()));
    }
    let record = NewSubmission {
        event_id: play.event.id,
        challenge_id: challenge.id,
        item_id: None,
        flag_id: None,
        scorer,
        user_id: player.id,
        ip: &ip.to_string(),
        value: &value,
        verdict,
        at: now,
    };
    let mut tx = crate::db::begin_write(&state.pool).await?;
    jeopardy_store::insert_submission(&mut tx, &record).await?;
    let mut hints_cost = 0;
    if solved {
        hints_cost = jeopardy_store::hints_cost(&mut tx, challenge.id, scorer).await?;
        jeopardy_store::insert_solve(&mut tx, &record, hints_cost).await?;
    }
    let stored = sqlx::query(
        "INSERT INTO code_submission (event_id, challenge_id, scorer_kind, scorer_id, user_id, language, source, passed, total, compile_error, at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(play.event.id)
    .bind(challenge.id)
    .bind(scorer.kind())
    .bind(scorer.id())
    .bind(player.id)
    .bind(&body.language)
    .bind(&body.source)
    .bind(i64::try_from(report.passed()).unwrap_or(0))
    .bind(i64::try_from(config.cases.len()).unwrap_or(0))
    .bind(report.compile_error.as_deref())
    .bind(now)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    tx.commit().await?;

    let points = if solved {
        state.notify(&play.event.slug);
        let board = Board::load(&state.pool, &play.event).await?;
        let solves = board
            .counts()
            .get(&(challenge.id as u64))
            .copied()
            .unwrap_or(0);
        Some(score_for_solve(
            current_value(&challenge.scoring, solves),
            &[hints_cost],
        ))
    } else {
        None
    };
    // The checks that follow need no write lock, and the lock is shared by the whole platform.
    drop(_guard);
    if solved {
        // Looking for a copy never fails the run: the player already has their answer.
        if let Err(e) = code::note_matches(
            &state,
            play.event.id,
            challenge.id,
            scorer,
            stored,
            &body.source,
        )
        .await
        {
            tracing::warn!("copy check failed: {e:?}");
        }
    }
    let counted = u32::from(!compile_failed);
    let mut out = code::report_json(&config, &report, false);
    out["verdict"] = json!(if solved {
        "correct"
    } else if compile_failed {
        "compile_error"
    } else {
        "incorrect"
    });
    out["correct"] = json!(solved);
    out["points"] = json!(points);
    out["attempts_remaining"] = json!(
        challenge
            .attempt_cap()
            .remaining(used.saturating_add(counted))
    );
    Ok(Json(out))
}

/// Sets the languages, tests and limits of a coding challenge.
pub async fn put_config(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(config): Json<CodeConfig>,
) -> Result<Json<CodeConfig>, ApiError> {
    let row = store::challenge_by_id(&state.pool, id).await?;
    if row.kind != "code" {
        return Err(ApiError::BadRequest(
            "only a coding challenge has tests".into(),
        ));
    }
    config.validate()?;
    code::put_config(&state.pool, id, &config).await?;
    Ok(Json(config))
}

/// An author tries a reference solution: nothing is recorded, and every test is shown.
pub async fn test_run(
    _: Require<needs::ManageChallenges>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RunBody>,
) -> Result<Json<Value>, ApiError> {
    check_source(&body.source)?;
    let row = store::challenge_by_id(&state.pool, id).await?;
    if row.kind != "code" {
        return Err(ApiError::BadRequest(
            "only a coding challenge has tests".into(),
        ));
    }
    let event = crate::event_store::by_id(&state.pool, row.event_id).await?;
    let config = code::config_of(&state.pool, id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("set the tests first".into()))?;
    let report = code::judge(
        &state,
        row.host(event.lab_host_id),
        &config,
        &body.language,
        &body.source,
    )
    .await?;
    Ok(Json(code::report_json(&config, &report, true)))
}
