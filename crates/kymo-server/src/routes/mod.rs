mod admin;
mod public;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use tower_http::trace::TraceLayer;

use crate::error::ApiError;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(public::login))
        .route("/auth/logout", post(public::logout))
        .route("/auth/me", get(public::me))
        .route("/ladder", get(public::ladder))
        .route(
            "/report/{code}",
            get(public::report_info).post(public::report_submit),
        )
        .route("/tournaments", get(public::list).post(admin::create))
        .route("/tournaments/{slug}", get(public::show))
        .route("/tournaments/{slug}/events", get(public::events))
        .route("/tournaments/{slug}/activity", get(public::activity))
        .route("/tournaments/{slug}/calendar.ics", get(public::calendar))
        .route("/settings", get(public::appearance))
        .route("/admin/demo", post(admin::load_demo))
        .route("/admin/import", post(admin::import))
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
            "/admin/webhooks",
            get(admin::list_webhooks).post(admin::create_webhook),
        )
        .route("/admin/webhooks/{id}", delete(admin::delete_webhook))
        .route("/admin/reports", get(admin::list_reports))
        .route("/admin/reports/{id}/approve", post(admin::approve_report))
        .route("/admin/reports/{id}/reject", post(admin::reject_report))
        .route(
            "/admin/tournaments/{id}/teams/{team}/code",
            post(admin::team_code),
        )
        .route("/admin/webhooks/{id}/test", post(admin::test_webhook))
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
        .route("/admin/tournaments/{id}/tiebreak", put(admin::set_tiebreak))
        .route("/admin/tournaments/{id}/accent", put(admin::set_accent))
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/schedule",
            put(admin::set_schedule),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/games",
            put(admin::set_games),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/live",
            put(admin::set_live),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/forfeit",
            put(admin::set_forfeit),
        );
    Router::new()
        .nest(
            "/api/v1",
            api.fallback(not_found)
                .layer(middleware::map_response(json_errors)),
        )
        .fallback(crate::assets::serve)
        .layer(middleware::from_fn(security_headers))
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

const CSP_BASE: &str = "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:";

/// Everything is denied framing except the embed pages, whose whole purpose is to sit in an
/// iframe on someone else's site or a stream overlay.
async fn security_headers(request: Request, next: Next) -> Response {
    let embeddable = request.uri().path().starts_with("/embed/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    let mut set = |name: &'static str, value: String| {
        if let Ok(value) = HeaderValue::from_str(&value) {
            headers
                .entry(HeaderName::from_static(name))
                .or_insert(value);
        }
    };
    set("x-content-type-options", "nosniff".into());
    set("referrer-policy", "same-origin".into());
    if embeddable {
        set(
            "content-security-policy",
            format!("{CSP_BASE}; frame-ancestors *"),
        );
    } else {
        set("x-frame-options", "DENY".into());
        set(
            "content-security-policy",
            format!("{CSP_BASE}; frame-ancestors 'none'"),
        );
    }
    response
}
