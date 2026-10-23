use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;

use crate::error::ApiError;
use crate::state::AppState;
use crate::tokens;

/// Extractor that only succeeds for requests carrying a valid organizer session token.
pub struct Admin;

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthorized)?;
        if tokens::is_valid(&state.pool, token).await? {
            Ok(Admin)
        } else {
            Err(ApiError::Unauthorized)
        }
    }
}

/// Like `Admin`, but refuses API tokens: a leaked automation token must not be able to mint
/// new ones that outlive its own revocation.
pub struct SessionAdmin;

impl FromRequestParts<AppState> for SessionAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthorized)?;
        if tokens::is_session(&state.pool, token).await? {
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

    async fn from_request_parts(parts: &mut Parts, _: &AppState) -> Result<Self, ApiError> {
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |info| info.0.ip());
        Ok(ClientIp(ip))
    }
}
