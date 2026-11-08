//! Challenge-based tournament matches over HTTP: settings and assignment for organizers,
//! the match view and answers for players.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use kymo_duel::Assignment;
use kymo_engine::Side;
use serde::Deserialize;

use crate::auth::{Account, ClientIp, Require, needs};
use crate::challenge_store as store;
use crate::db::{self, ChallengeSettings, Tournament};
use crate::duel::{self, Finish, MatchView, Submission, SubmitOut};
use crate::duel_store as dstore;
use crate::error::ApiError;
use crate::event_store;
use crate::monitor_store;
use crate::state::AppState;

fn bad(message: &str) -> ApiError {
    ApiError::BadRequest(message.into())
}

#[derive(Deserialize)]
pub struct SettingsBody {
    assignment: Assignment,
    solo: u8,
    team: u8,
    #[serde(default)]
    seed: i64,
    /// Seconds a match stays open once it starts; none for no limit.
    time_limit: Option<i64>,
}

pub async fn save_settings(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SettingsBody>,
) -> Result<Json<Tournament>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    kymo_duel::check_count(usize::from(body.solo)).map_err(|e| bad(&e.to_string()))?;
    kymo_duel::check_count(usize::from(body.team)).map_err(|e| bad(&e.to_string()))?;
    if body
        .time_limit
        .is_some_and(|l| !(60..=7 * 24 * 3600).contains(&l))
    {
        return Err(bad("the time limit is between 1 minute and 7 days"));
    }
    if body.assignment != Assignment::None && t.best_of > 1 {
        return Err(ApiError::Conflict(
            "challenge matches are single games: set the tournament to best of 1 first".into(),
        ));
    }
    let answered: i64 = sqlx_count(&state, id).await?;
    if answered > 0 && body.assignment != t.challenges.assignment {
        return Err(ApiError::Conflict(
            "players have answered already, so the assignment mode can no longer change".into(),
        ));
    }
    let settings = ChallengeSettings {
        assignment: body.assignment,
        solo: body.solo,
        team: body.team,
        seed: body.seed,
        time_limit: body.time_limit,
    };
    dstore::save_settings(&state.pool, id, &settings).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "tournament_challenges_set",
        "tournament",
        id,
        &format!(
            "assignment {}, {} solo, {} team",
            body.assignment.as_str(),
            body.solo,
            body.team
        ),
    )
    .await?;
    state.notify(&t.slug);
    Ok(Json(db::get(&state.pool, id).await?))
}

async fn sqlx_count(state: &AppState, tournament_id: i64) -> Result<i64, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM duel_answer WHERE tournament_id = ?")
            .bind(tournament_id)
            .fetch_one(&state.pool)
            .await?,
    )
}

#[derive(Deserialize)]
pub struct AssignBody {
    challenge_ids: Vec<i64>,
}

/// Picks the challenges of one match by hand.
pub async fn assign(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
    Json(body): Json<AssignBody>,
) -> Result<Json<MatchView>, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    if t.challenges.assignment == Assignment::None {
        return Err(ApiError::Conflict(
            "this tournament does not use challenges".into(),
        ));
    }
    let event = event_store::by_id(&state.pool, t.event_id.ok_or(ApiError::NotFound)?).await?;
    let ready = t.bracket.as_ref().is_some_and(|b| {
        b.matches
            .get(match_id)
            .is_some_and(|m| m.is_ready() && !m.is_played())
    });
    if !ready {
        return Err(ApiError::Conflict(
            "this match is not waiting to be played".into(),
        ));
    }
    let want = duel::expected_count(&t, &event);
    let ids = body.challenge_ids.clone();
    if ids.len() != want || ids.iter().collect::<std::collections::HashSet<_>>().len() != want {
        return Err(bad(&format!(
            "pick exactly {want} different challenges for this match"
        )));
    }
    for cid in &ids {
        let row = store::challenge_by_id(&state.pool, *cid)
            .await
            .map_err(|_| bad("unknown challenge"))?;
        if row.event_id != event.id || row.state != "published" {
            return Err(bad("pick published challenges of this tournament"));
        }
    }
    dstore::assign(&state.pool, id, match_id as i64, &ids).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "match_challenges_set",
        "match",
        format!("{id}:{match_id}"),
        &format!("{ids:?}"),
    )
    .await?;
    drop(_guard);
    Ok(Json(
        duel::view(&state, &t, &event, match_id, None, true).await?,
    ))
}

/// What an organizer sees of a match: both sides, every challenge and every judged answer.
pub async fn staff_view(
    _: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
) -> Result<Json<MatchView>, ApiError> {
    let t = db::get(&state.pool, id).await?;
    let (t, event) = duel::context(&state, &t.slug, match_id).await?;
    Ok(Json(
        duel::view(&state, &t, &event, match_id, None, true).await?,
    ))
}

#[derive(Deserialize, Default)]
pub struct FinalizeBody {
    /// Decides a tie nothing else can break: "a" or "b".
    winner: Option<String>,
}

/// Closes the match with what was earned so far.
pub async fn finalize(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
    body: Option<Json<FinalizeBody>>,
) -> Result<Json<MatchView>, ApiError> {
    let winner = match body.and_then(|b| b.0.winner).as_deref() {
        None => None,
        Some("a") => Some(Side::A),
        Some("b") => Some(Side::B),
        Some(_) => return Err(bad("the winner is a or b")),
    };
    let t = db::get(&state.pool, id).await?;
    let (t, event) = duel::context(&state, &t.slug, match_id).await?;
    duel::ensure_assigned(&state, &t, &event, match_id).await?;
    match duel::settle(&state, id, match_id, true, winner).await? {
        Finish::NeedsDecision => {
            return Err(ApiError::Conflict(
                "the match is tied with nothing to break it: choose a winner".into(),
            ));
        }
        Finish::NotYet => {
            return Err(ApiError::Conflict(
                "this match cannot be finalized: it has no challenges or is already decided".into(),
            ));
        }
        Finish::Done => {}
    }
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "match_finalized",
        "match",
        format!("{id}:{match_id}"),
        winner.map_or("by score", |_| "winner chosen by an organizer"),
    )
    .await?;
    let t = db::get(&state.pool, id).await?;
    Ok(Json(
        duel::view(&state, &t, &event, match_id, None, true).await?,
    ))
}

/// Forgets a match's answers and state so it can be played again. The result must be cleared first.
pub async fn restart(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path((id, match_id)): Path<(i64, usize)>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.write_lock.lock().await;
    let t = db::get(&state.pool, id).await?;
    if t.bracket
        .as_ref()
        .is_some_and(|b| b.matches.get(match_id).is_some_and(|m| m.is_played()))
    {
        return Err(ApiError::Conflict("clear the match result first".into()));
    }
    dstore::clear_match(&state.pool, id, match_id as i64).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "match_restarted",
        "match",
        format!("{id}:{match_id}"),
        "answers cleared",
    )
    .await?;
    state.notify(&t.slug);
    Ok(StatusCode::NO_CONTENT)
}

/// Fills the roster from the event's registrations and links teams to their players.
pub async fn fill_roster(
    who: Require<needs::ManageEvents>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Tournament>, ApiError> {
    let t = duel::fill_roster(&state, id).await?;
    monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "roster_from_registrations",
        "tournament",
        id,
        &format!("{} teams", t.teams.len()),
    )
    .await?;
    Ok(Json(t))
}

pub async fn play(
    Account(player, _): Account<needs::PlayEvent>,
    State(state): State<AppState>,
    Path((slug, match_id)): Path<(String, usize)>,
) -> Result<Json<MatchView>, ApiError> {
    let (t, event) = duel::context(&state, &slug, match_id).await?;
    let you = duel::viewer_side(&state, &t, &event, match_id, Some(player.id)).await?;
    if you.is_none() {
        return Err(ApiError::Denied("you are not playing in this match".into()));
    }
    let view = duel::view(&state, &t, &event, match_id, you, false).await?;
    // A time limit that passed while nobody was answering still ends the match.
    if view.status == "needs_decision" {
        let _ = duel::settle(&state, t.id, match_id, false, None).await;
    }
    Ok(Json(view))
}

#[derive(Deserialize)]
pub struct AnswerBody {
    value: String,
    item_id: Option<i64>,
}

pub async fn answer(
    Account(player, _): Account<needs::SubmitFlag>,
    ClientIp(ip): ClientIp,
    State(state): State<AppState>,
    Path((slug, match_id, challenge_id)): Path<(String, usize, i64)>,
    Json(body): Json<AnswerBody>,
) -> Result<Json<SubmitOut>, ApiError> {
    let out = duel::submit(
        &state,
        &slug,
        match_id,
        &Submission {
            user_id: player.id,
            ip: &ip.to_string(),
            challenge_id,
            item_id: body.item_id,
            value: &body.value,
        },
    )
    .await?;
    Ok(Json(out))
}

/// The matches this account can play right now: ready, not decided, on one of its sides.
pub async fn mine(
    Account(player, _): Account<needs::PlayEvent>,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let t = db::get_by_slug(&state.pool, &slug).await?;
    let (Some(event_id), Some(bracket)) = (t.event_id, t.bracket.as_ref()) else {
        return Ok(Json(serde_json::json!({ "matches": [] })));
    };
    if t.challenges.assignment == Assignment::None {
        return Ok(Json(serde_json::json!({ "matches": [] })));
    }
    let event = event_store::by_id(&state.pool, event_id).await?;
    let mut matches = Vec::new();
    for m in bracket
        .matches
        .iter()
        .filter(|m| m.is_ready() && !m.is_played())
    {
        if duel::viewer_side(&state, &t, &event, m.id, Some(player.id))
            .await?
            .is_some()
        {
            matches.push(m.id);
        }
    }
    Ok(Json(serde_json::json!({ "matches": matches })))
}
