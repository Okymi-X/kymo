use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::{Extensions, HeaderMap};

use kymo_core::{Permission, Role};

use crate::error::ApiError;
use crate::state::AppState;
use crate::tokens::{self, Access};

fn bearer(parts: &Parts) -> Result<&str, ApiError> {
    parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)
}

async fn access_of(parts: &Parts, state: &AppState) -> Result<Access, ApiError> {
    tokens::access(&state.pool, bearer(parts)?)
        .await?
        .ok_or(ApiError::Unauthorized)
}

/// Extractor that only succeeds for the organizer: the legacy password session, an API token,
/// or a user session whose account role is admin.
pub struct Admin;

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let access = access_of(parts, state).await?;
        if state.can(access.role(), Permission::ManageSettings).await? {
            Ok(Admin)
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

/// Who is calling and with which role. `user_id` is empty for the legacy organizer token.
#[derive(Debug, Clone, Copy)]
pub struct Caller {
    pub user_id: Option<i64>,
    pub role: Role,
}

/// A permission that an extractor can demand. One marker type per permission.
pub trait Needs {
    const PERMISSION: Permission;
}

pub mod needs {
    use super::{Needs, Permission};

    macro_rules! marker {
        ($($name:ident),*) => {$(
            pub struct $name;
            impl Needs for $name {
                const PERMISSION: Permission = Permission::$name;
            }
        )*};
    }

    marker!(
        ManageEvents,
        ManageChallenges,
        ManageUsers,
        ManageSettings,
        ManagePermissions,
        Monitor,
        ModerateBans,
        ViewAudit,
        Referee,
        PlayEvent,
        ManageTeam,
        SubmitFlag,
        StartInstance
    );
}

/// Succeeds when the caller's role holds the permission named by `P`, for any kind of token.
/// The decision comes from the configured matrix (`AppState::can`), never from handlers.
pub struct Require<P: Needs>(pub Caller, PhantomData<P>);

impl<P: Needs> FromRequestParts<AppState> for Require<P> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let access = access_of(parts, state).await?;
        let caller = Caller {
            user_id: access.user.map(|u| u.id),
            role: access.role(),
        };
        if state.can(caller.role, P::PERMISSION).await? {
            Ok(Require(caller, PhantomData))
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

/// The caller when a valid token is sent, or none for an anonymous visitor. A token that is
/// sent but invalid is still a 401, so a stale session is not mistaken for a visitor.
pub struct MaybeCaller(pub Option<Caller>);

impl FromRequestParts<AppState> for MaybeCaller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        if !parts.headers.contains_key(AUTHORIZATION) {
            return Ok(MaybeCaller(None));
        }
        let access = access_of(parts, state).await?;
        Ok(MaybeCaller(Some(Caller {
            user_id: access.user.map(|u| u.id),
            role: access.role(),
        })))
    }
}

/// Any signed in, not banned player account.
pub struct Player {
    pub id: i64,
    pub role: Role,
}

impl FromRequestParts<AppState> for Player {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        match access_of(parts, state).await?.user {
            Some(user) => Ok(Player {
                id: user.id,
                role: user.role,
            }),
            None => Err(ApiError::Forbidden),
        }
    }
}

/// A signed in account whose role holds the permission named by `P`: `Player` plus the check,
/// so an organizer can switch what players may do (submit flags, start instances, manage a
/// team) without any route knowing about roles.
pub struct Account<P: Needs>(pub Player, pub PhantomData<P>);

impl<P: Needs> FromRequestParts<AppState> for Account<P> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let player = Player::from_request_parts(parts, state).await?;
        if state.can(player.role, P::PERMISSION).await? {
            Ok(Account(player, PhantomData))
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

/// A caller who may record results and schedules: the organizer, or a referee token for the
/// tournaments it was issued for.
pub struct Official {
    scope: Option<Vec<i64>>,
}

impl Official {
    /// Fails with 403 when a referee token was not issued for this tournament.
    pub fn allows(&self, tournament_id: i64) -> Result<(), ApiError> {
        match &self.scope {
            Some(ids) if !ids.contains(&tournament_id) => Err(ApiError::Forbidden),
            _ => Ok(()),
        }
    }
}

impl FromRequestParts<AppState> for Official {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let access = access_of(parts, state).await?;
        let role = access.role();
        if !state.can(role, Permission::Referee).await? {
            return Err(ApiError::Forbidden);
        }
        // Only roles that manage events see every tournament. A referee is limited to the
        // scope of its token; a referee account has none yet, so it sees no tournament.
        let scope = (!state.can(role, Permission::ManageEvents).await?).then_some(access.scope);
        Ok(Official { scope })
    }
}

/// Like `Admin`, but refuses API tokens: a leaked automation token must not be able to mint
/// new ones that outlive its own revocation.
pub struct SessionAdmin;

impl FromRequestParts<AppState> for SessionAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let access = access_of(parts, state).await?;
        if access.session && state.can(access.role(), Permission::ManageSettings).await? {
            Ok(SessionAdmin)
        } else {
            Err(ApiError::Unauthorized)
        }
    }
}

/// The peer address of the connection, or the unspecified address when it is not known
/// (for example in tests that call the router directly).
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        Ok(ClientIp(client_ip(
            &parts.headers,
            &parts.extensions,
            state.trusts_proxy(),
        )))
    }
}

/// The caller's address. Behind one trusted reverse proxy the proxy appends the real client
/// as the last `X-Forwarded-For` entry; earlier entries are client supplied and ignored.
pub fn client_ip(headers: &HeaderMap, extensions: &Extensions, trust_proxy: bool) -> IpAddr {
    if trust_proxy
        && let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
    {
        return ip;
    }
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |info| info.0.ip())
}
