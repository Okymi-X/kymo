use axum::Json;
use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use kymo_engine::EngineError;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    Forbidden,
    Banned,
    NotFound,
    Conflict(String),
    TooManyRequests,
    /// 403 with a specific reason, for example not being registered for an event.
    Denied(String),
    /// 429 for the flag submission limit, with the seconds until the next try.
    RateLimited {
        retry_after: i64,
    },
    /// 429 from a monitor rate rule, with the seconds until the next try.
    Throttled {
        retry_after: i64,
    },
    /// 403 for a banned network address.
    AddressBanned,
    /// 503 for a missing or failing backend such as the container platform.
    Unavailable(String),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut retry = None;
        let (status, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication required".into()),
            ApiError::Forbidden => (
                StatusCode::FORBIDDEN,
                "you do not have permission to do that".into(),
            ),
            ApiError::Banned => (StatusCode::FORBIDDEN, "this account is banned".into()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many failed sign ins, try again in a minute".into(),
            ),
            ApiError::Denied(m) => (StatusCode::FORBIDDEN, m),
            ApiError::RateLimited { retry_after } => {
                retry = Some(retry_after.max(1));
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    format!(
                        "too many submissions, try again in {} seconds",
                        retry_after.max(1)
                    ),
                )
            }
            ApiError::Throttled { retry_after } => {
                retry = Some(retry_after.max(1));
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    format!(
                        "too many requests, try again in {} seconds",
                        retry_after.max(1)
                    ),
                )
            }
            ApiError::AddressBanned => (StatusCode::FORBIDDEN, "this address is banned".into()),
            ApiError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        let mut response = (status, Json(json!({ "error": message }))).into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let value = retry.map_or_else(|| HeaderValue::from_static("60"), HeaderValue::from);
            response.headers_mut().insert(RETRY_AFTER, value);
        }
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => ApiError::NotFound,
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApiError::Conflict("that name is already taken".into())
            }
            _ => {
                tracing::error!(%err, "database error");
                ApiError::Internal
            }
        }
    }
}

impl From<EngineError> for ApiError {
    fn from(err: EngineError) -> Self {
        match err {
            EngineError::UnknownMatch(_) => ApiError::NotFound,
            EngineError::NotReady(_) | EngineError::DownstreamPlayed | EngineError::NoResult(_) => {
                ApiError::Conflict(err.to_string())
            }
            _ => ApiError::BadRequest(err.to_string()),
        }
    }
}
