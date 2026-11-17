//! What happens to a player who submits a dynamic flag that was generated for somebody else.
//! The event decides: nothing, a warning and a report for the organizers, or a warning and then
//! an automatic ban. Every offence counts as a strike; the first `warnings` strikes only warn.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::monitor_alerts;
use crate::monitor_store as store;
use crate::state::AppState;

pub const MAX_WARNINGS: u32 = 10;

/// What the event does once a player is past their warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingAction {
    /// Strikes are not counted and the player is not told anything.
    Off,
    /// The player is warned, then reported to the organizers on every further strike.
    Manual,
    /// The player is warned, then banned.
    AutoBan,
    /// The player is warned, then disqualified from this event: off its scoreboard and out of its
    /// play, the account untouched. A team is disqualified as a whole.
    Disqualify,
}

/// What the player is told after a submission of somebody else's flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingStatus {
    Warning,
    Reported,
    Banned,
    Disqualified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SharingNotice {
    pub status: SharingStatus,
    /// Offences by this player in this event, this one included.
    pub strikes: u32,
    /// Offences that only warn.
    pub warnings: u32,
}

/// Records one offence and applies the event's rule. None when the rule is off.
pub async fn strike(
    state: &AppState,
    event_id: i64,
    user_id: i64,
    challenge_id: i64,
    action: SharingAction,
    warnings: u32,
) -> Result<Option<SharingNotice>, ApiError> {
    if action == SharingAction::Off {
        return Ok(None);
    }
    let now = state.now();
    sqlx::query(
        "INSERT INTO sharing_strike (event_id, user_id, challenge_id, at) VALUES (?, ?, ?, ?)",
    )
    .bind(event_id)
    .bind(user_id)
    .bind(challenge_id)
    .bind(now)
    .execute(&state.pool)
    .await?;
    let counted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sharing_strike WHERE event_id = ? AND user_id = ?",
    )
    .bind(event_id)
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;
    let strikes = u32::try_from(counted).unwrap_or(u32::MAX);
    let notice = |status| SharingNotice {
        status,
        strikes,
        warnings,
    };
    if strikes <= warnings {
        return Ok(Some(notice(SharingStatus::Warning)));
    }
    let detail =
        format!("submitted flags generated for other players {strikes} times in event {event_id}");
    if action == SharingAction::Disqualify {
        let reason = format!("Flag sharing: {detail}");
        if disqualify(state, event_id, user_id, &reason, None).await? {
            return Ok(Some(notice(SharingStatus::Disqualified)));
        }
    }
    if action == SharingAction::AutoBan {
        let reason = format!("Flag sharing: {detail}");
        if monitor_alerts::ban_player(state, user_id, &reason, None, now)
            .await?
            .is_some()
        {
            return Ok(Some(notice(SharingStatus::Banned)));
        }
    }
    // Manual mode, or a player who cannot be banned (staff, already banned): leave it to people.
    store::audit(
        &state.pool,
        now,
        None,
        "flag_sharing_review",
        "user",
        user_id,
        &detail,
    )
    .await?;
    Ok(Some(notice(SharingStatus::Reported)))
}

/// Disqualifies the participant a user plays as in an event (the user in a solo event, their team
/// in a team event). False when they are not registered or already disqualified.
pub async fn disqualify(
    state: &AppState,
    event_id: i64,
    user_id: i64,
    reason: &str,
    actor: Option<&crate::auth::Caller>,
) -> Result<bool, ApiError> {
    let event = crate::event_store::by_id(&state.pool, event_id).await?;
    let Ok(scorer) = crate::jeopardy_store::scorer_of(&state.pool, &event, user_id).await else {
        return Ok(false);
    };
    let (user, team) = match scorer {
        crate::jeopardy_store::Scorer::User(id) => (Some(id), None),
        crate::jeopardy_store::Scorer::Team(id) => (None, Some(id)),
    };
    let now = state.now();
    let done = sqlx::query(
        "UPDATE event_participant SET disqualified_at = ?, disqualified_reason = ? WHERE event_id = ? AND disqualified_at IS NULL AND (user_id = ? OR team_id = ?)",
    )
    .bind(now)
    .bind(reason.chars().take(200).collect::<String>())
    .bind(event_id)
    .bind(user)
    .bind(team)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if done == 0 {
        return Ok(false);
    }
    store::audit(
        &state.pool,
        now,
        actor,
        "participant_disqualify",
        "event",
        event_id,
        &format!("user {user_id}: {reason}"),
    )
    .await?;
    state.notify(&event.slug);
    Ok(true)
}
