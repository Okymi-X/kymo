use std::str::FromStr;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use kymo_core::Role;
use serde::{Deserialize, Serialize};

use crate::auth::{ClientIp, Player, Require, needs};
use crate::error::ApiError;
use crate::passwords;
use crate::state::AppState;
use crate::team_store;
use crate::tokens;
use crate::users::{self, UserOut, UserPage};

#[derive(Deserialize)]
pub struct RegisterBody {
    username: String,
    password: String,
    email: Option<String>,
}

#[derive(Serialize)]
pub struct SessionOut {
    token: String,
    user: UserOut,
}

/// Creates an account and signs it in.
pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterBody>,
) -> Result<(StatusCode, Json<SessionOut>), ApiError> {
    if !users::registration_open(&state.pool).await? {
        return Err(ApiError::Denied(
            "sign-ups are closed on this server, ask an organizer to create an account for you"
                .into(),
        ));
    }
    let username = users::check_username(&body.username)?;
    passwords::check_strength(&body.password)?;
    let email = body
        .email
        .as_deref()
        .filter(|e| !e.trim().is_empty())
        .map(users::check_email)
        .transpose()?;
    let hash = passwords::hash(body.password).await?;
    let id = users::create(&state.pool, username, email.as_deref(), &hash, state.now()).await?;
    let token = tokens::issue_user(&state.pool, id).await?;
    let user = UserOut::try_from(users::by_id(&state.pool, id).await?)?;
    Ok((StatusCode::CREATED, Json(SessionOut { token, user })))
}

#[derive(Deserialize)]
pub struct PlayerLoginBody {
    username: String,
    password: String,
}

pub async fn player_login(
    State(state): State<AppState>,
    ClientIp(client): ClientIp,
    Json(body): Json<PlayerLoginBody>,
) -> Result<Json<SessionOut>, ApiError> {
    if state.login_blocked(client) {
        return Err(ApiError::TooManyRequests);
    }
    let found = users::by_username(&state.pool, &body.username).await?;
    // Unknown usernames still pay for one hash check, so timing does not reveal them.
    let hash = found
        .as_ref()
        .map_or_else(|| state.decoy_hash(), |u| u.password_hash.clone());
    let matches = passwords::verify(hash, body.password).await;
    let user = found.filter(|_| matches);
    state.record_login(client, user.is_some());
    let Some(user) = user else {
        return Err(ApiError::Unauthorized);
    };
    if user.banned {
        return Err(ApiError::Banned);
    }
    let token = tokens::issue_user(&state.pool, user.id).await?;
    Ok(Json(SessionOut {
        token,
        user: UserOut::try_from(user)?,
    }))
}

#[derive(Serialize)]
pub struct MeOut {
    #[serde(flatten)]
    user: UserOut,
    team_id: Option<i64>,
}

async fn me_out(state: &AppState, id: i64) -> Result<MeOut, ApiError> {
    let team_id = team_store::of_user(&state.pool, id).await?.map(|t| t.id);
    let user = UserOut::try_from(users::by_id(&state.pool, id).await?)?;
    Ok(MeOut { user, team_id })
}

pub async fn me(State(state): State<AppState>, player: Player) -> Result<Json<MeOut>, ApiError> {
    Ok(Json(me_out(&state, player.id).await?))
}

#[derive(Deserialize)]
pub struct UpdateMeBody {
    display_name: Option<String>,
    bio: Option<String>,
    current_password: Option<String>,
    new_password: Option<String>,
}

pub async fn update_me(
    State(state): State<AppState>,
    player: Player,
    headers: HeaderMap,
    Json(body): Json<UpdateMeBody>,
) -> Result<Json<MeOut>, ApiError> {
    let display_name = body
        .display_name
        .as_deref()
        .map(users::check_display_name)
        .transpose()?;
    let bio = body.bio.as_deref().map(users::check_bio).transpose()?;
    let new_hash = match (&body.current_password, body.new_password) {
        (None, None) => None,
        (Some(current), Some(new)) => {
            passwords::check_strength(&new)?;
            let user = users::by_id(&state.pool, player.id).await?;
            if !passwords::verify(user.password_hash, current.clone()).await {
                return Err(ApiError::Denied("the current password is not right".into()));
            }
            Some(passwords::hash(new).await?)
        }
        _ => {
            return Err(ApiError::BadRequest(
                "changing the password needs current_password and new_password".into(),
            ));
        }
    };
    users::update_profile(
        &state.pool,
        player.id,
        display_name.as_deref(),
        bio.as_deref(),
    )
    .await?;
    if let Some(hash) = new_hash {
        users::set_password(&state.pool, player.id, &hash).await?;
        let keep = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        tokens::end_user_sessions(&state.pool, player.id, keep).await?;
    }
    Ok(Json(me_out(&state, player.id).await?))
}

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

const MAX_PER_PAGE: i64 = 100;

pub async fn list_users(
    _: Require<needs::ManageUsers>,
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<UserPage>, ApiError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, MAX_PER_PAGE);
    let q = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    Ok(Json(users::list(&state.pool, q, page, per_page).await?))
}

#[derive(Deserialize)]
pub struct CreateUserBody {
    username: String,
    email: Option<String>,
    role: Option<String>,
    /// Left out, the server makes a temporary password and returns it once.
    password: Option<String>,
}

#[derive(Serialize)]
pub struct CreatedUser {
    user: UserOut,
    /// Only present when the server made the password. It is never shown again.
    temporary_password: Option<String>,
}

/// Letters and digits without the ones that are easy to misread.
const PASSWORD_ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

fn temporary_password() -> String {
    (0..16)
        .map(|_| {
            PASSWORD_ALPHABET[usize::from(rand::random::<u8>()) % PASSWORD_ALPHABET.len()] as char
        })
        .collect()
}

/// Lets an admin make an account directly, so sign-ups can stay closed.
pub async fn create_user(
    admin: Require<needs::ManageUsers>,
    State(state): State<AppState>,
    Json(body): Json<CreateUserBody>,
) -> Result<(StatusCode, Json<CreatedUser>), ApiError> {
    let username = users::check_username(&body.username)?;
    let email = body
        .email
        .as_deref()
        .filter(|e| !e.trim().is_empty())
        .map(users::check_email)
        .transpose()?;
    let role = match body.role.as_deref() {
        None | Some("") => Role::Player,
        Some(r) => Role::from_str(r)
            .ok()
            .filter(|r| *r != Role::Visitor)
            .ok_or_else(|| {
                ApiError::BadRequest("role is player, referee, monitor or admin".into())
            })?,
    };
    let (password, temporary) = match body.password.filter(|p| !p.is_empty()) {
        Some(p) => (p, None),
        None => {
            let p = temporary_password();
            (p.clone(), Some(p))
        }
    };
    passwords::check_strength(&password)?;
    let hash = passwords::hash(password).await?;
    let id = users::create(&state.pool, username, email.as_deref(), &hash, state.now()).await?;
    if role != Role::Player {
        users::moderate(&state.pool, id, Some(role), None).await?;
    }
    crate::monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&admin.0),
        "user_create",
        "user",
        id,
        &format!("username={username} role={}", role.as_str()),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedUser {
            user: UserOut::try_from(users::by_id(&state.pool, id).await?)?,
            temporary_password: temporary,
        }),
    ))
}

#[derive(Deserialize)]
pub struct ModerateBody {
    role: Option<String>,
    banned: Option<bool>,
    display_name: Option<String>,
    bio: Option<String>,
    /// An address, or an empty string to remove it.
    email: Option<String>,
    /// A new password the organizer chose. The account is signed out everywhere.
    new_password: Option<String>,
}

pub async fn update_user(
    admin: Require<needs::ManageUsers>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ModerateBody>,
) -> Result<Json<UserOut>, ApiError> {
    let role = body
        .role
        .as_deref()
        .map(Role::from_str)
        .transpose()
        .map_err(|_| ApiError::BadRequest("role is player, referee, monitor or admin".into()))?;
    if role == Some(Role::Visitor) {
        return Err(ApiError::BadRequest(
            "role is player, referee, monitor or admin".into(),
        ));
    }
    if admin.0.user_id == Some(id) && (role.is_some() || body.banned.is_some()) {
        return Err(ApiError::Conflict(
            "you cannot change your own role or ban yourself".into(),
        ));
    }
    // Check everything first so a refused part changes nothing.
    let display_name = body
        .display_name
        .as_deref()
        .map(users::check_display_name)
        .transpose()?;
    let bio = body.bio.as_deref().map(users::check_bio).transpose()?;
    let email = match body.email.as_deref().map(str::trim) {
        None => None,
        Some("") => Some(None),
        Some(raw) => Some(Some(users::check_email(raw)?)),
    };
    let new_hash = match &body.new_password {
        Some(password) => {
            passwords::check_strength(password)?;
            Some(passwords::hash(password.clone()).await?)
        }
        None => None,
    };
    users::by_id(&state.pool, id).await?;
    users::moderate(&state.pool, id, role, body.banned).await?;
    users::update_profile(&state.pool, id, display_name.as_deref(), bio.as_deref()).await?;
    if let Some(email) = &email {
        users::set_email(&state.pool, id, email.as_deref()).await?;
    }
    if let Some(hash) = &new_hash {
        users::set_password(&state.pool, id, hash).await?;
        tokens::end_user_sessions(&state.pool, id, None).await?;
    }
    let mut detail = format!(
        "role={} banned={}",
        role.map_or("unchanged", Role::as_str),
        body.banned
            .map_or("unchanged".to_owned(), |b| b.to_string())
    );
    // Say what was edited, never the values (an email or a password).
    for (name, edited) in [
        ("display_name", display_name.is_some()),
        ("bio", bio.is_some()),
        ("email", email.is_some()),
        ("password_reset", new_hash.is_some()),
    ] {
        if edited {
            detail.push_str(&format!(" {name}=changed"));
        }
    }
    if body.banned.is_some() {
        crate::vpn::sync_quiet(&state).await;
    }
    crate::monitor_store::audit(
        &state.pool,
        state.now(),
        Some(&admin.0),
        "user_update",
        "user",
        id,
        &detail,
    )
    .await?;
    Ok(Json(UserOut::try_from(
        users::by_id(&state.pool, id).await?,
    )?))
}

#[derive(Serialize, Deserialize)]
pub struct RegistrationBody {
    open: bool,
}

pub async fn get_registration(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<Json<RegistrationBody>, ApiError> {
    Ok(Json(RegistrationBody {
        open: users::registration_open(&state.pool).await?,
    }))
}

pub async fn set_registration(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Json(body): Json<RegistrationBody>,
) -> Result<Json<RegistrationBody>, ApiError> {
    users::set_registration_open(&state.pool, body.open).await?;
    Ok(Json(body))
}

#[derive(Serialize)]
pub struct PublicRegistration {
    open: bool,
    /// How to reach an organizer, empty when none was set.
    contact: String,
}

/// Whether sign-ups are open, and who to ask when they are not, so pages can say so before
/// anyone fills a form in.
pub async fn registration_status(
    State(state): State<AppState>,
) -> Result<Json<PublicRegistration>, ApiError> {
    Ok(Json(PublicRegistration {
        open: users::registration_open(&state.pool).await?,
        contact: users::contact(&state.pool).await?,
    }))
}

#[derive(Serialize, Deserialize)]
pub struct ContactBody {
    contact: String,
}

pub async fn get_contact(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
) -> Result<Json<ContactBody>, ApiError> {
    Ok(Json(ContactBody {
        contact: users::contact(&state.pool).await?,
    }))
}

pub async fn set_contact(
    _: Require<needs::ManageSettings>,
    State(state): State<AppState>,
    Json(body): Json<ContactBody>,
) -> Result<Json<ContactBody>, ApiError> {
    Ok(Json(ContactBody {
        contact: users::set_contact(&state.pool, &body.contact).await?,
    }))
}
