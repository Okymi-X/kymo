//! Flag submission flow. Every rule comes from kymo-core (time) and kymo-jeopardy (flags,
//! rate limit, attempt cap, scoring); this module only orders the checks and stores the result.

use kymo_core::{EventState, can_submit};
use kymo_jeopardy::{Verdict, check, current_value, score_for_solve};

use crate::challenge_store::{self, ChallengeRow};
use crate::error::ApiError;
use crate::event_store::{self, EventRow};
use crate::jeopardy_store::{self, Board, NewSubmission, Scorer};
use crate::state::AppState;

pub const MAX_SUBMITTED_FLAG: usize = 500;

/// A registered participant playing a Jeopardy event.
pub struct Play {
    pub event: EventRow,
    pub scorer: Scorer,
    pub effective: EventState,
}

/// Resolves the event by slug and the caller's scoring participant. Drafts and other event
/// kinds are 404, an unregistered caller is 403.
pub async fn play_context(state: &AppState, slug: &str, user_id: i64) -> Result<Play, ApiError> {
    let event = event_store::by_slug(&state.pool, slug).await?;
    if event.kind != "jeopardy" || event.stored_state()? == EventState::Draft {
        return Err(ApiError::NotFound);
    }
    let scorer = jeopardy_store::scorer_of(&state.pool, &event, user_id).await?;
    let effective = event.effective_state(state.now())?;
    Ok(Play {
        event,
        scorer,
        effective,
    })
}

/// A challenge of this event that players may know about: never a draft.
pub async fn challenge_of(
    state: &AppState,
    play: &Play,
    challenge_id: i64,
) -> Result<ChallengeRow, ApiError> {
    let row = challenge_store::challenge_by_id(&state.pool, challenge_id).await?;
    if row.event_id != play.event.id || row.state == "draft" {
        return Err(ApiError::NotFound);
    }
    Ok(row)
}

/// A challenge before its release time does not exist for players.
pub fn released(row: &ChallengeRow, now: i64) -> Result<(), ApiError> {
    if row.release_at.is_some_and(|r| now < r) {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

/// Whether the challenge accepts submissions or hint purchases right now.
pub fn open_for_play(play: &Play, row: &ChallengeRow, now: i64) -> Result<(), ApiError> {
    can_submit(play.effective, Some(row.window()), now)
        .map_err(|denied| ApiError::Conflict(denied.message().into()))
}

pub fn verdict_str(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Correct => "correct",
        Verdict::Incorrect => "incorrect",
        Verdict::BadFormat => "bad_format",
    }
}

pub struct Attempt<'a> {
    pub user_id: i64,
    pub ip: String,
    pub flag: &'a str,
}

pub struct Outcome {
    pub verdict: Verdict,
    pub points: Option<i64>,
    pub attempts_remaining: Option<u32>,
}

/// Checks the flag and records the submission, and the solve when it is correct.
pub async fn run(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
    attempt: &Attempt<'_>,
) -> Result<Outcome, ApiError> {
    if attempt.flag.chars().count() > MAX_SUBMITTED_FLAG {
        return Err(ApiError::BadRequest("that flag is too long".into()));
    }
    let now = state.now();
    open_for_play(play, challenge, now)?;
    let _guard = state.write_lock.lock().await;
    let scorer = play.scorer;
    if jeopardy_store::is_solved(&state.pool, challenge.id, scorer.id()).await? {
        return Err(ApiError::Conflict("already solved".into()));
    }
    let verdict = check(
        &challenge.flag,
        &challenge.format(),
        challenge.case_sensitive,
        attempt.flag,
    );
    let mut used = 0;
    if verdict.counts_as_attempt() {
        let settings = jeopardy_store::settings(&state.pool, play.event.id).await?;
        state
            .rate_check(play.event.id, scorer.id(), &settings.rate_rule())
            .map_err(|denied| ApiError::RateLimited {
                retry_after: denied.retry_after_secs,
            })?;
        used = jeopardy_store::attempts_used(&state.pool, challenge.id, scorer).await?;
        challenge
            .attempt_cap()
            .check(used)
            .map_err(|capped| ApiError::Conflict(capped.to_string()))?;
    }

    let record = NewSubmission {
        event_id: play.event.id,
        challenge_id: challenge.id,
        scorer,
        user_id: attempt.user_id,
        ip: &attempt.ip,
        value: attempt.flag.trim(),
        verdict: verdict_str(verdict),
        at: now,
    };
    let mut tx = state.pool.begin().await?;
    jeopardy_store::insert_submission(&mut tx, &record).await?;
    let mut hints_cost = 0;
    if verdict == Verdict::Correct {
        hints_cost = jeopardy_store::hints_cost(&mut tx, challenge.id, scorer).await?;
        jeopardy_store::insert_solve(&mut tx, &record, hints_cost).await?;
    }
    tx.commit().await?;

    let points = if verdict == Verdict::Correct {
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
    let counted = u32::from(verdict.counts_as_attempt());
    Ok(Outcome {
        verdict,
        points,
        attempts_remaining: challenge
            .attempt_cap()
            .remaining(used.saturating_add(counted)),
    })
}
