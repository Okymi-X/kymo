mod admin;
mod public;

use axum::Router;
use axum::body::Body;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(public::login))
        .route("/auth/logout", post(public::logout))
        .route("/tournaments", get(public::list).post(admin::create))
        .route("/tournaments/{slug}", get(public::show))
        .route("/tournaments/{slug}/events", get(public::events))
        .route("/tournaments/{slug}/activity", get(public::activity))
        .route("/settings", get(public::appearance))
        .route("/admin/demo", post(admin::load_demo))
        .route(
            "/admin/settings",
            put(admin::save_appearance).delete(admin::reset_appearance),
        )
        .route(
            "/admin/tokens",
            get(admin::list_tokens).post(admin::create_token),
        )
        .route("/admin/tokens/{id}", delete(admin::revoke_token))
        .route(
            "/admin/tournaments/{id}",
            put(admin::rename).delete(admin::remove),
        )
        .route("/admin/tournaments/{id}/teams", put(admin::set_teams))
        .route("/admin/tournaments/{id}/start", post(admin::start))
        .route("/admin/tournaments/{id}/reset", post(admin::reset))
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/result",
            put(admin::set_result),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/result",
            delete(admin::clear_result),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/forfeit",
            put(admin::set_forfeit),
        );
    let header = |name: &'static str, value: &'static str| {
        SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        )
    };
    Router::new()
        .nest("/api/v1", api.fallback(not_found).layer(middleware::map_response(json_errors)))
        .fallback(crate::assets::serve)
        .layer(header("x-content-type-options", "nosniff"))
        .layer(header("x-frame-options", "DENY"))
        .layer(header("referrer-policy", "same-origin"))
        .layer(header(
            "content-security-policy",
            "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; frame-ancestors 'none'",
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn not_found() -> ApiError {
    ApiError::NotFound
}

/// Framework rejections (bad JSON, wrong method, oversized body) come back as plain text or
/// empty bodies. Replace them with the API's JSON envelope and a generic message, so clients
/// get one error shape and no parser internals.
async fn json_errors(response: Response<Body>) -> Response<Body> {
    let status = response.status();
    let is_error = status.is_client_error() || status.is_server_error();
    let is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if !is_error || is_json {
        return response;
    }
    let message = match status {
        StatusCode::METHOD_NOT_ALLOWED => "method not allowed",
        StatusCode::PAYLOAD_TOO_LARGE => "request body is too large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "send the body as application/json",
        StatusCode::NOT_FOUND => "not found",
        s if s.is_server_error() => "internal error",
        _ => "invalid request",
    };
    (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
}
