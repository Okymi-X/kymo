//! Flag submission flow. Every rule comes from kymo-core (time) and kymo-jeopardy (flags,
//! rate limit, attempt cap, scoring); this module only orders the checks and stores the result.

use kymo_core::{EventState, can_submit};
use kymo_jeopardy::{FlagSpec, Verdict, check, current_value, score_for_solve};

use crate::challenge_store::{self, ChallengeRow};
use crate::error::ApiError;
use crate::event_store::{self, EventRow};
use crate::flag_set;
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

/// Like `play_context`, but a tournament event is accepted too. Only for the routes that are
/// gated per match afterwards (instances and file downloads): the challenge board and flag
/// submission stay Jeopardy-only.
pub async fn play_context_any(
    state: &AppState,
    slug: &str,
    user_id: i64,
) -> Result<Play, ApiError> {
    let event = event_store::by_slug(&state.pool, slug).await?;
    if !matches!(event.kind.as_str(), "jeopardy" | "tournament")
        || event.stored_state()? == EventState::Draft
    {
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
    /// Flags found and flags in all, for a challenge with several flags.
    pub flags: Option<(u32, u32)>,
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
    if challenge.kind == "dfir" {
        return Err(ApiError::BadRequest(
            "this challenge is answered question by question".into(),
        ));
    }
    let now = state.now();
    open_for_play(play, challenge, now)?;
    let _guard = state.write_lock.lock().await;
    let scorer = play.scorer;
    if jeopardy_store::is_solved(&state.pool, challenge.id, scorer.id()).await? {
        return Err(ApiError::Conflict("already solved".into()));
    }
    if challenge.multi_flag {
        return run_multi(state, play, challenge, attempt, now).await;
    }
    let dynamic = challenge.flag_mode == "dynamic";
    let verdict = if dynamic {
        judge_dynamic(state, challenge, scorer, attempt.flag).await?
    } else {
        check(
            &challenge.flag,
            &challenge.format(),
            challenge.case_sensitive,
            attempt.flag,
        )
    };
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
        item_id: None,
        flag_id: None,
        scorer,
        user_id: attempt.user_id,
        ip: &attempt.ip,
        value: attempt.flag.trim(),
        verdict: verdict_str(verdict),
        at: now,
    };
    let mut tx = crate::db::begin_write(&state.pool).await?;
    jeopardy_store::insert_submission(&mut tx, &record).await?;
    let mut hints_cost = 0;
    if verdict == Verdict::Correct {
        hints_cost = jeopardy_store::hints_cost(&mut tx, challenge.id, scorer).await?;
        jeopardy_store::insert_solve(&mut tx, &record, hints_cost).await?;
    }
    tx.commit().await?;
    if dynamic && verdict == Verdict::Incorrect {
        note_sharing(state, challenge, scorer, attempt).await?;
    }

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
        flags: None,
    })
}

/// Judges one answer to a DFIR question and records it. A correct answer scores the question's
/// own points; a question is answered once and its attempt limit is its own.
pub async fn run_item(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
    item_id: i64,
    attempt: &Attempt<'_>,
) -> Result<Outcome, ApiError> {
    if attempt.flag.chars().count() > MAX_SUBMITTED_FLAG {
        return Err(ApiError::BadRequest("that answer is too long".into()));
    }
    if challenge.kind != "dfir" {
        return Err(ApiError::BadRequest(
            "this challenge has no questions".into(),
        ));
    }
    let now = state.now();
    open_for_play(play, challenge, now)?;
    let item = crate::duel_store::items(&state.pool, challenge.id)
        .await?
        .into_iter()
        .find(|i| i.id == item_id)
        .ok_or(ApiError::NotFound)?;
    let _guard = state.write_lock.lock().await;
    let scorer = play.scorer;
    if jeopardy_store::items_solved(&state.pool, play.event.id, scorer)
        .await?
        .contains_key(&item.id)
    {
        return Err(ApiError::Conflict("already answered correctly".into()));
    }
    let verdict = check(
        &item.answer,
        &kymo_jeopardy::FlagFormat::default(),
        item.case_sensitive,
        attempt.flag,
    );
    let settings = jeopardy_store::settings(&state.pool, play.event.id).await?;
    state
        .rate_check(play.event.id, scorer.id(), &settings.rate_rule())
        .map_err(|denied| ApiError::RateLimited {
            retry_after: denied.retry_after_secs,
        })?;
    let used = jeopardy_store::item_attempts_used(&state.pool, item.id, scorer).await?;
    let cap = kymo_jeopardy::AttemptCap {
        max: item.max_attempts.and_then(|m| u32::try_from(m).ok()),
    };
    cap.check(used)
        .map_err(|capped| ApiError::Conflict(capped.to_string()))?;
    let record = NewSubmission {
        event_id: play.event.id,
        challenge_id: challenge.id,
        item_id: Some(item.id),
        flag_id: None,
        scorer,
        user_id: attempt.user_id,
        ip: &attempt.ip,
        value: attempt.flag.trim(),
        verdict: verdict_str(verdict),
        at: now,
    };
    let mut tx = crate::db::begin_write(&state.pool).await?;
    jeopardy_store::insert_submission(&mut tx, &record).await?;
    if verdict == Verdict::Correct {
        jeopardy_store::insert_item_solve(&mut tx, &record, item.points).await?;
    }
    tx.commit().await?;
    if verdict == Verdict::Correct {
        state.notify(&play.event.slug);
    }
    Ok(Outcome {
        verdict,
        points: (verdict == Verdict::Correct).then_some(item.points),
        attempts_remaining: cap.remaining(used.saturating_add(1)),
        flags: None,
    })
}

/// A spec that never matches, used to ask only whether a submission has the accepted shape.
fn shape_only() -> FlagSpec {
    FlagSpec::PerTeam {
        expected: String::new(),
    }
}

/// Judges a submission against the flag generated for this participant's own runs. Every run's
/// flag is compared, with no early exit, so timing does not say which run matched.
async fn judge_dynamic(
    state: &AppState,
    challenge: &ChallengeRow,
    scorer: Scorer,
    submitted: &str,
) -> Result<Verdict, ApiError> {
    let format = challenge.format();
    let shape = check(&shape_only(), &format, challenge.case_sensitive, submitted);
    if shape == Verdict::BadFormat {
        return Ok(shape);
    }
    let own = flag_set::run_values(&state.pool, challenge.id, scorer).await?;
    let mut correct = false;
    for value in own.values().flatten() {
        let spec = FlagSpec::PerTeam {
            expected: value.clone(),
        };
        correct |= check(&spec, &format, challenge.case_sensitive, submitted) == Verdict::Correct;
    }
    Ok(if correct {
        Verdict::Correct
    } else {
        Verdict::Incorrect
    })
}

/// A wrong flag that was generated for somebody else is a sign of sharing: leave a note for the
/// organizers. The flag value is never written to the note.
async fn note_sharing(
    state: &AppState,
    challenge: &ChallengeRow,
    scorer: Scorer,
    attempt: &Attempt<'_>,
) -> Result<(), ApiError> {
    let Some((kind, id)) =
        flag_set::owner_of_value(&state.pool, challenge.id, attempt.flag.trim()).await?
    else {
        return Ok(());
    };
    if kind == scorer.kind() && id == scorer.id() {
        return Ok(());
    }
    crate::monitor_store::audit_from(
        &state.pool,
        state.now(),
        Some(attempt.user_id),
        "player",
        Some(&attempt.ip),
        "flag_shared",
        "challenge",
        challenge.id,
        &format!(
            "submitted a flag that was generated for {kind} {id} on challenge {}",
            challenge.id
        ),
    )
    .await
}

/// Submission on a challenge with several flags: one input, each flag found once, partial
/// credit per flag, and the challenge is solved with its last flag.
async fn run_multi(
    state: &AppState,
    play: &Play,
    challenge: &ChallengeRow,
    attempt: &Attempt<'_>,
    now: i64,
) -> Result<Outcome, ApiError> {
    let scorer = play.scorer;
    let flags = challenge_store::flag_rows(&state.pool, challenge.id).await?;
    let total = u32::try_from(flags.len()).unwrap_or(u32::MAX);
    let format = challenge.format();
    let submitted = attempt.flag.trim();
    let shape = check(&shape_only(), &format, challenge.case_sensitive, submitted);
    let dynamic = challenge.flag_mode == "dynamic";
    let own = if dynamic {
        flag_set::run_values(&state.pool, challenge.id, scorer).await?
    } else {
        Default::default()
    };
    let have = flag_set::found(&state.pool, challenge.id, scorer).await?;

    // Compare against every flag, found or not, with no early exit.
    let mut matched: Option<i64> = None;
    if shape != Verdict::BadFormat {
        for flag in &flags {
            let candidates: Vec<String> = if dynamic {
                own.get(&flag.id).cloned().unwrap_or_default()
            } else {
                vec![flag.value.clone()]
            };
            for value in candidates {
                let spec = FlagSpec::PerTeam { expected: value };
                if check(&spec, &format, challenge.case_sensitive, submitted) == Verdict::Correct {
                    matched = Some(flag.id);
                }
            }
        }
    }
    if matched.is_some_and(|id| have.contains_key(&id)) {
        return Err(ApiError::Conflict("you already have this flag".into()));
    }
    let verdict = match (shape, matched) {
        (Verdict::BadFormat, _) => Verdict::BadFormat,
        (_, Some(_)) => Verdict::Correct,
        _ => Verdict::Incorrect,
    };

    // Only wrong tries count against the cap here: a right flag is progress, not a try.
    let found_now = u32::try_from(have.len()).unwrap_or(u32::MAX);
    let mut wrong = 0;
    if verdict.counts_as_attempt() {
        let settings = jeopardy_store::settings(&state.pool, play.event.id).await?;
        state
            .rate_check(play.event.id, scorer.id(), &settings.rate_rule())
            .map_err(|denied| ApiError::RateLimited {
                retry_after: denied.retry_after_secs,
            })?;
        let used = jeopardy_store::attempts_used(&state.pool, challenge.id, scorer).await?;
        wrong = used.saturating_sub(found_now);
        challenge
            .attempt_cap()
            .check(wrong)
            .map_err(|capped| ApiError::Conflict(capped.to_string()))?;
    }

    let record = NewSubmission {
        event_id: play.event.id,
        challenge_id: challenge.id,
        item_id: None,
        flag_id: matched,
        scorer,
        user_id: attempt.user_id,
        ip: &attempt.ip,
        value: submitted,
        verdict: verdict_str(verdict),
        at: now,
    };
    let mut tx = crate::db::begin_write(&state.pool).await?;
    jeopardy_store::insert_submission(&mut tx, &record).await?;
    let mut award = None;
    let mut found_after = found_now;
    if let (Verdict::Correct, Some(flag_id)) = (verdict, matched) {
        let weights: Vec<i64> = flags.iter().map(|f| f.weight).collect();
        let shares =
            kymo_jeopardy::dynamic::split_points(current_value(&challenge.scoring, 0), &weights);
        let base_of = |id: i64| {
            flags
                .iter()
                .position(|f| f.id == id)
                .and_then(|i| shares.get(i))
                .copied()
                .unwrap_or(0)
        };
        let hints = jeopardy_store::hints_cost(&mut tx, challenge.id, scorer).await?;
        let paid: i64 = have.iter().map(|(id, points)| base_of(*id) - points).sum();
        let (points, _) = kymo_jeopardy::dynamic::charge_hint_cost(base_of(flag_id), hints - paid);
        flag_set::insert_flag_solve(&mut tx, &record, flag_id, points).await?;
        found_after = found_now + 1;
        if found_after == total {
            jeopardy_store::insert_solve(&mut tx, &record, 0).await?;
        }
        award = Some(points);
    }
    tx.commit().await?;
    if award.is_none() && verdict == Verdict::Incorrect {
        note_sharing(state, challenge, scorer, attempt).await?;
    }
    if award.is_some() {
        state.notify(&play.event.slug);
    }
    let wrong_now = wrong.saturating_add(u32::from(verdict == Verdict::Incorrect));
    Ok(Outcome {
        verdict,
        points: award,
        attempts_remaining: challenge.attempt_cap().remaining(wrong_now),
        flags: Some((found_after, total)),
    })
}
