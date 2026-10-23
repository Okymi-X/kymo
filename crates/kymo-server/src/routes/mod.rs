mod admin;
mod public;

use axum::Router;
use axum::http::{HeaderName, HeaderValue};
use axum::routing::{delete, get, post, put};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(public::login))
        .route("/tournaments", get(public::list).post(admin::create))
        .route("/tournaments/{slug}", get(public::show))
        .route("/tournaments/{slug}/events", get(public::events))
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
        );
    let header = |name: &'static str, value: &'static str| {
        SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        )
    };
    Router::new()
        .nest("/api/v1", api)
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
