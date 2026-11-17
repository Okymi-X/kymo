//! What an organizer can do about cheating in one event: see who has been caught sharing flags,
//! forgive a strike, remove a participant from the event (and let them back), and, with the users
//! page, ban the account. The steps are ordered from the gentlest to the hardest.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{Require, needs};
use crate::error::ApiError;
use crate::monitor_store as store;
use crate::state::AppState;
use crate::{challenge_store, event_store, jeopardy_store, sharing};

type StrikeRow = (i64, String, String, bool, i64, i64, String);

/// Players with at least one strike in this event, worst first, with where each stands.
pub async fn list(
    _: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let event = event_store::by_id(&state.pool, id).await?;
    let rows: Vec<StrikeRow> = sqlx::query_as(
        "SELECT s.user_id, u.username, u.display_name, u.banned, COUNT(*), MAX(s.at), GROUP_CONCAT(DISTINCT s.challenge_id) FROM sharing_strike s JOIN user u ON u.id = s.user_id WHERE s.event_id = ? GROUP BY s.user_id ORDER BY COUNT(*) DESC, MAX(s.at) DESC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let titles: std::collections::HashMap<i64, String> =
        challenge_store::challenges(&state.pool, id)
            .await?
            .into_iter()
            .map(|c| (c.id, c.title))
            .collect();
    let mut players = Vec::new();
    for (user_id, username, display_name, banned, strikes, last_at, challenge_ids) in rows {
        let (disqualified_at, reason) =
            match jeopardy_store::scorer_of(&state.pool, &event, user_id).await {
                Ok(scorer) => disqualification(&state, id, scorer).await?,
                Err(_) => (None, String::new()),
            };
        let challenges: Vec<Value> = challenge_ids
            .split(',')
            .filter_map(|c| c.parse::<i64>().ok())
            .map(|c| json!({ "id": c, "title": titles.get(&c).cloned().unwrap_or_default() }))
            .collect();
        players.push(json!({
            "user_id": user_id,
            "username": username,
            "display_name": display_name,
            "banned": banned,
            "strikes": strikes,
            "last_at": last_at,
            "challenges": challenges,
            "disqualified_at": disqualified_at,
            "disqualified_reason": reason,
        }));
    }
    Ok(Json(json!({ "players": players })))
}

async fn disqualification(
    state: &AppState,
    event_id: i64,
    scorer: jeopardy_store::Scorer,
) -> Result<(Option<i64>, String), ApiError> {
    let (user, team) = match scorer {
        jeopardy_store::Scorer::User(id) => (Some(id), None),
        jeopardy_store::Scorer::Team(id) => (None, Some(id)),
    };
    let row: Option<(Option<i64>, String)> = sqlx::query_as(
        "SELECT disqualified_at, disqualified_reason FROM event_participant WHERE event_id = ? AND (user_id = ? OR team_id = ?)",
    )
    .bind(event_id)
    .bind(user)
    .bind(team)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.unwrap_or((None, String::new())))
}

#[derive(Deserialize)]
pub struct DisqualifyBody {
    user_id: i64,
    #[serde(default)]
    reason: String,
}

/// Removes the participant a user plays as from the event: off the scoreboard, no more play.
pub async fn disqualify(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<DisqualifyBody>,
) -> Result<StatusCode, ApiError> {
    let reason = body.reason.trim();
    if reason.is_empty() || reason.chars().count() > 200 {
        return Err(ApiError::BadRequest(
            "say why, in up to 200 characters".into(),
        ));
    }
    event_store::by_id(&state.pool, id).await?;
    if !sharing::disqualify(&state, id, body.user_id, reason, Some(&who.0)).await? {
        return Err(ApiError::Conflict(
            "that player is not registered in this event, or is already disqualified".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Lets a disqualified participant back in. Their solves count again.
pub async fn reinstate(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    let event = event_store::by_id(&state.pool, id).await?;
    let scorer = jeopardy_store::scorer_of(&state.pool, &event, user_id).await?;
    let (user, team) = match scorer {
        jeopardy_store::Scorer::User(i) => (Some(i), None),
        jeopardy_store::Scorer::Team(i) => (None, Some(i)),
    };
    let done = sqlx::query("UPDATE event_participant SET disqualified_at = NULL, disqualified_reason = '' WHERE event_id = ? AND disqualified_at IS NOT NULL AND (user_id = ? OR team_id = ?)")
        .bind(id)
        .bind(user)
        .bind(team)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if done == 0 {
        return Err(ApiError::NotFound);
    }
    store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "participant_reinstate",
        "event",
        id,
        &format!("user {user_id}"),
    )
    .await?;
    state.notify(&event.slug);
    Ok(StatusCode::NO_CONTENT)
}

/// Clears a player's strikes in this event, for a mistake or a first offence forgiven.
pub async fn forgive(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    event_store::by_id(&state.pool, id).await?;
    let done = sqlx::query("DELETE FROM sharing_strike WHERE event_id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if done == 0 {
        return Err(ApiError::NotFound);
    }
    store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "strikes_forgiven",
        "event",
        id,
        &format!("user {user_id}: {done} strike(s)"),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Accepted coding solutions that look copied from each other, most alike first. Dismissed ones
/// stay out unless asked for.
pub async fn code_matches(
    _: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    event_store::by_id(&state.pool, id).await?;
    let rows: Vec<(i64, i64, String, String, i64, i64)> = sqlx::query_as(
        "SELECT m.id, m.challenge_id, m.name, m.other_name, m.score, m.at FROM code_match m WHERE m.event_id = ? AND m.status = 'open' ORDER BY m.score DESC, m.at DESC LIMIT 200",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let titles: std::collections::HashMap<i64, String> =
        challenge_store::challenges(&state.pool, id)
            .await?
            .into_iter()
            .map(|c| (c.id, c.title))
            .collect();
    let matches: Vec<Value> = rows
        .into_iter()
        .map(|(mid, challenge_id, name, other_name, score, at)| {
            json!({ "id": mid, "challenge": titles.get(&challenge_id).cloned().unwrap_or_default(), "name": name, "other_name": other_name, "score": score, "at": at })
        })
        .collect();
    Ok(Json(json!({ "matches": matches })))
}

/// Both sources of a match, to read side by side.
pub async fn code_match(
    _: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    type Row = (
        String,
        String,
        i64,
        String,
        String,
        String,
        String,
        String,
        i64,
    );
    let row: Option<Row> = sqlx::query_as(
        "SELECT m.name, m.other_name, m.score, a.language, a.source, b.language, b.source, ch.title, m.at FROM code_match m JOIN code_submission a ON a.id = m.submission_id JOIN code_submission b ON b.id = m.other_submission_id JOIN challenge ch ON ch.id = m.challenge_id WHERE m.id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let (name, other_name, score, language, source, other_language, other_source, challenge, at) =
        row.ok_or(ApiError::NotFound)?;
    Ok(Json(json!({
        "challenge": challenge, "score": score, "at": at,
        "a": { "name": name, "language": language, "source": source },
        "b": { "name": other_name, "language": other_language, "source": other_source },
    })))
}

/// A false alarm: two honest solutions of a short problem. It leaves the list.
pub async fn dismiss_code_match(
    who: Require<needs::ModerateBans>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let done =
        sqlx::query("UPDATE code_match SET status = 'dismissed' WHERE id = ? AND status = 'open'")
            .bind(id)
            .execute(&state.pool)
            .await?
            .rows_affected();
    if done == 0 {
        return Err(ApiError::NotFound);
    }
    store::audit(
        &state.pool,
        state.now(),
        Some(&who.0),
        "code_match_dismissed",
        "code_match",
        id,
        "",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
