//! Sign in for the organizer console. It is a different door from the player sign in: only an
//! account that is not a plain player gets a session here, so the console never opens for a
//! player who guessed its address, and a failed attempt says nothing about which half was wrong.

use axum::Json;
use axum::extract::State;
use kymo_core::{Permission, Role};
use serde::{Deserialize, Serialize};

use crate::auth::ClientIp;
use crate::error::ApiError;
use crate::monitor_store;
use crate::passwords;
use crate::state::AppState;
use crate::tokens::{self, Kind};
use crate::users;

#[derive(Deserialize)]
pub struct OrganizerLoginBody {
    /// An organizer, monitor or referee account. Empty uses the organizer password the server
    /// was started with.
    #[serde(default)]
    username: String,
    password: String,
}

#[derive(Serialize)]
pub struct OrganizerSession {
    token: String,
    role: Role,
    username: Option<String>,
    user_id: Option<i64>,
    permissions: Vec<Permission>,
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(client): ClientIp,
    Json(body): Json<OrganizerLoginBody>,
) -> Result<Json<OrganizerSession>, ApiError> {
    if state.login_blocked(client) {
        return Err(ApiError::TooManyRequests);
    }
    let username = body.username.trim();
    let (token, role, name, user_id) = if username.is_empty() {
        let ok = state.verify_password(body.password).await;
        state.record_login(client, ok);
        if !ok {
            return Err(ApiError::Unauthorized);
        }
        let token = tokens::issue(&state.pool, Kind::Session, "", None).await?;
        (token, Role::Admin, None, None)
    } else {
        let found = users::by_username(&state.pool, username).await?;
        // Unknown names still pay for one hash check, so timing does not reveal them.
        let hash = found
            .as_ref()
            .map_or_else(|| state.decoy_hash(), |u| u.password_hash.clone());
        let matches = passwords::verify(hash, body.password).await;
        let user = users::UserOut::try_from(found.filter(|_| matches).ok_or_else(|| {
            state.record_login(client, false);
            ApiError::Unauthorized
        })?)?;
        // A banned account and a player account look the same as a wrong password.
        if user.banned || matches!(user.role, Role::Player | Role::Visitor) {
            state.record_login(client, false);
            return Err(ApiError::Unauthorized);
        }
        state.record_login(client, true);
        let token = tokens::issue_user(&state.pool, user.id).await?;
        (token, user.role, Some(user.username), Some(user.id))
    };
    let ip = client.to_string();
    monitor_store::audit_from(
        &state.pool,
        state.now(),
        user_id,
        role.as_str(),
        Some(&ip),
        "organizer_login",
        "session",
        name.as_deref().unwrap_or("organizer password"),
        "",
    )
    .await?;
    let permissions = state.permission_matrix().await?.granted(role);
    Ok(Json(OrganizerSession {
        token,
        role,
        username: name,
        user_id,
        permissions,
    }))
}
