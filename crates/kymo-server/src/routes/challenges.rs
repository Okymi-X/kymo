//! Player side of Jeopardy: the challenge board, hints, downloads and flag submission.
//! The flag, its pattern and the scoring settings are never part of these responses.

use std::collections::HashMap;
use std::convert::Infallible;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_core::Stream;
use kymo_core::{EventState, can_view_challenges};
use kymo_jeopardy::{Verdict, current_value, score_for_solve};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::{Account, ClientIp, needs};
use crate::challenge_store::{self as store, CategoryRow};
use crate::error::ApiError;
use crate::event_store;
use crate::jeopardy_store::{self, Board};
use crate::state::AppState;
use crate::submit::{self, Attempt, challenge_of, open_for_play, play_context, released};

#[derive(Serialize)]
struct HintOut {
    id: i64,
    cost: i64,
    unlocked: bool,
    content: Option<String>,
}

#[derive(Serialize)]
struct AttachmentOut {
    id: i64,
    name: String,
    mime: String,
    size: i64,
    sha256: String,
    url: String,
}

#[derive(Serialize)]
struct FlagFormatOut {
    prefix: String,
    suffix: String,
}

#[derive(Serialize)]
struct ChallengeOut {
    id: i64,
    category_id: i64,
    title: String,
    description: String,
    /// What a solve is worth right now, before this participant's hint costs.
    points: i64,
    solved: bool,
    solved_at: Option<i64>,
    solved_by: u32,
    /// "open" accepts submissions, "closed" is shown but does not (retired, paused, ended).
    status: &'static str,
    release_at: Option<i64>,
    retire_at: Option<i64>,
    flag_format: FlagFormatOut,
    max_attempts: Option<i64>,
    attempts_remaining: Option<u32>,
    requires_instance: bool,
    /// Where its instance runs (the challenge's own choice, else the event's): which VPN profile it needs.
    lab_host_id: Option<i64>,
    /// `flag` (also instance backed) or `dfir`, answered question by question.
    kind: String,
    items: Vec<ItemOut>,
    hints: Vec<HintOut>,
    attachments: Vec<AttachmentOut>,
}

#[derive(Serialize)]
struct ItemOut {
    id: i64,
    question: String,
    points: i64,
    solved: bool,
    attempts_remaining: Option<u32>,
}

#[derive(Serialize)]
pub struct ChallengeList {
    event: Value,
    categories: Vec<CategoryRow>,
    challenges: Vec<ChallengeOut>,
}

fn view_gate(effective: EventState) -> Result<(), ApiError> {
    can_view_challenges(effective).map_err(|denied| ApiError::Conflict(denied.message().into()))
}

pub async fn list(
    Account(player, _): Account<needs::PlayEvent>,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ChallengeList>, ApiError> {
    let play = play_context(&state, &slug, player.id).await?;
    view_gate(play.effective)?;
    let now = state.now();
    let event_id = play.event.id;
    let board = Board::load(&state.pool, &play.event).await?;
    let counts = board.counts();
    let hints = store::hints_of_event(&state.pool, event_id).await?;
    let files = store::attachments_of_event(&state.pool, event_id).await?;
    let unlocked = jeopardy_store::unlocked_hints(&state.pool, play.scorer).await?;
    let solved_at: HashMap<i64, i64> = board
        .solves_of(play.scorer.id())
        .map(|s| (s.challenge_id, s.at))
        .collect();
    let answered = jeopardy_store::items_solved(&state.pool, event_id, play.scorer).await?;
    let all_items = crate::duel_store::items_of_event(&state.pool, event_id).await?;
    let mut challenges = Vec::new();
    for row in store::challenges(&state.pool, event_id).await? {
        if row.state == "draft" || released(&row, now).is_err() {
            continue;
        }
        let dfir = row.kind == "dfir";
        let mut items = Vec::new();
        if dfir {
            for item in all_items.iter().filter(|i| i.challenge_id == row.id) {
                let used =
                    jeopardy_store::item_attempts_used(&state.pool, item.id, play.scorer).await?;
                items.push(ItemOut {
                    id: item.id,
                    question: item.question.clone(),
                    points: item.points,
                    solved: answered.contains_key(&item.id),
                    attempts_remaining: item
                        .max_attempts
                        .and_then(|m| u32::try_from(m).ok())
                        .map(|m| m.saturating_sub(used)),
                });
            }
        }
        let all_answered = dfir && !items.is_empty() && items.iter().all(|i| i.solved);
        let solved_by = if dfir {
            // Participants who answered every question.
            let ids: Vec<u64> = all_items
                .iter()
                .filter(|i| i.challenge_id == row.id)
                .map(|i| jeopardy_store::ITEM_BASE + i.id as u64)
                .collect();
            let mut per: HashMap<u64, usize> = HashMap::new();
            for s in board.solves.iter().filter(|s| ids.contains(&s.challenge)) {
                *per.entry(s.participant).or_default() += 1;
            }
            u32::try_from(
                per.values()
                    .filter(|n| **n == ids.len() && !ids.is_empty())
                    .count(),
            )
            .unwrap_or(0)
        } else {
            counts.get(&(row.id as u64)).copied().unwrap_or(0)
        };
        let attempts_remaining = match row.max_attempts {
            Some(_) => {
                let used = jeopardy_store::attempts_used(&state.pool, row.id, play.scorer).await?;
                row.attempt_cap().remaining(used)
            }
            None => None,
        };
        let host = row.host(play.event.lab_host_id);
        challenges.push(ChallengeOut {
            points: current_value(&row.scoring, solved_by),
            solved: if dfir {
                all_answered
            } else {
                solved_at.contains_key(&row.id)
            },
            solved_at: if dfir {
                all_answered
                    .then(|| answered.values().copied().max())
                    .flatten()
            } else {
                solved_at.get(&row.id).copied()
            },
            kind: row.kind.clone(),
            items,
            solved_by,
            status: if open_for_play(&play, &row, now).is_ok() {
                "open"
            } else {
                "closed"
            },
            flag_format: FlagFormatOut {
                prefix: row.flag_prefix.clone(),
                suffix: row.flag_suffix.clone(),
            },
            attempts_remaining,
            hints: hints
                .iter()
                .filter(|h| h.challenge_id == row.id)
                .map(|h| {
                    let open = unlocked.contains(&h.id);
                    HintOut {
                        id: h.id,
                        cost: h.cost,
                        unlocked: open,
                        content: open.then(|| h.content.clone()),
                    }
                })
                .collect(),
            attachments: files
                .iter()
                .filter(|a| a.challenge_id == row.id)
                .map(|a| AttachmentOut {
                    id: a.id,
                    name: a.name.clone(),
                    mime: a.mime.clone(),
                    size: a.size,
                    sha256: a.sha256.clone(),
                    url: format!(
                        "/api/v1/events/{slug}/challenges/{}/attachments/{}",
                        row.id, a.id
                    ),
                })
                .collect(),
            id: row.id,
            category_id: row.category_id,
            title: row.title,
            description: row.description,
            release_at: row.release_at,
            retire_at: row.retire_at,
            max_attempts: row.max_attempts,
            requires_instance: row.requires_instance,
            lab_host_id: host,
        });
    }
    Ok(Json(ChallengeList {
        event: json!({
            "id": event_id,
            "slug": play.event.slug,
            "name": play.event.name,
            "mode": play.event.mode,
            "effective_state": play.effective,
        }),
        categories: store::categories(&state.pool, event_id).await?,
        challenges,
    }))
}

pub async fn unlock_hint(
    Account(player, _): Account<needs::PlayEvent>,
    State(state): State<AppState>,
    Path((slug, challenge_id, hint_id)): Path<(String, i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    let play = play_context(&state, &slug, player.id).await?;
    let now = state.now();
    let _guard = state.write_lock.lock().await;
    let challenge = challenge_of(&state, &play, challenge_id).await?;
    released(&challenge, now)?;
    open_for_play(&play, &challenge, now)?;
    let hint = store::hint_by_id(&state.pool, hint_id).await?;
    if hint.challenge_id != challenge.id {
        return Err(ApiError::NotFound);
    }
    if jeopardy_store::is_solved(&state.pool, challenge.id, play.scorer.id()).await? {
        return Err(ApiError::Conflict("already solved".into()));
    }
    let fresh = jeopardy_store::unlock_hint(&state.pool, play.scorer, hint.id, now).await?;
    Ok(Json(json!({
        "hint_id": hint.id,
        "cost": hint.cost,
        "content": hint.content,
        "newly_unlocked": fresh,
    })))
}

pub async fn download(
    Account(player, _): Account<needs::PlayEvent>,
    State(state): State<AppState>,
    Path((slug, challenge_id, attachment_id)): Path<(String, i64, i64)>,
) -> Result<Response, ApiError> {
    let play = crate::submit::play_context_any(&state, &slug, player.id).await?;
    view_gate(play.effective)?;
    let challenge = challenge_of(&state, &play, challenge_id).await?;
    released(&challenge, state.now())?;
    crate::duel::player_gate(&state, &play.event, challenge.id, player.id, false).await?;
    let meta = store::attachment_meta(&state.pool, attachment_id).await?;
    if meta.challenge_id != challenge.id {
        return Err(ApiError::NotFound);
    }
    let data = store::attachment_data(&state.pool, attachment_id).await?;
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", meta.name))
        .map_err(|_| ApiError::Internal)?;
    let mime = HeaderValue::from_str(&meta.mime).map_err(|_| ApiError::Internal)?;
    let mut response = data.into_response();
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, mime);
    headers.insert(CONTENT_DISPOSITION, disposition);
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

#[derive(Deserialize)]
pub struct SubmitBody {
    flag: String,
}

pub async fn submit(
    Account(player, _): Account<needs::SubmitFlag>,
    ClientIp(ip): ClientIp,
    State(state): State<AppState>,
    Path((slug, challenge_id)): Path<(String, i64)>,
    Json(body): Json<SubmitBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let play = play_context(&state, &slug, player.id).await?;
    let challenge = challenge_of(&state, &play, challenge_id).await?;
    let attempt = Attempt {
        user_id: player.id,
        ip: ip.to_string(),
        flag: &body.flag,
    };
    let outcome = submit::run(&state, &play, &challenge, &attempt).await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "verdict": submit::verdict_str(outcome.verdict),
            "correct": outcome.verdict == Verdict::Correct,
            "points": outcome.points,
            "attempts_remaining": outcome.attempts_remaining,
        })),
    ))
}

/// Answers one question of a DFIR challenge.
pub async fn submit_item(
    Account(player, _): Account<needs::SubmitFlag>,
    ClientIp(ip): ClientIp,
    State(state): State<AppState>,
    Path((slug, challenge_id, item_id)): Path<(String, i64, i64)>,
    Json(body): Json<SubmitBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let play = play_context(&state, &slug, player.id).await?;
    let challenge = challenge_of(&state, &play, challenge_id).await?;
    let attempt = Attempt {
        user_id: player.id,
        ip: ip.to_string(),
        flag: &body.flag,
    };
    let outcome = submit::run_item(&state, &play, &challenge, item_id, &attempt).await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "verdict": submit::verdict_str(outcome.verdict),
            "correct": outcome.verdict == Verdict::Correct,
            "points": outcome.points,
            "attempts_remaining": outcome.attempts_remaining,
        })),
    ))
}

/// Server sent events for an event: a message each time the board changes.
pub async fn stream(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let row = event_store::by_slug(&state.pool, &slug).await?;
    if row.stored_state()? == EventState::Draft {
        return Err(ApiError::NotFound);
    }
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(move |msg| match msg {
        Ok(changed) if changed == slug => {
            Some(Ok(Event::default().event("update").data("changed")))
        }
        _ => None,
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Points of one solve at the current challenge value.
pub fn solve_points(scoring: &kymo_jeopardy::Scoring, solved_by: u32, hints_cost: i64) -> i64 {
    score_for_solve(current_value(scoring, solved_by), &[hints_cost])
}
