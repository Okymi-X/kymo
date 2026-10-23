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
    NotFound,
    Conflict(String),
    TooManyRequests,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication required".into()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many failed sign ins, try again in a minute".into(),
            ),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        let mut response = (status, Json(json!({ "error": message }))).into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from_static("60"));
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
