mod admin;
mod public;

use axum::Router;
use axum::routing::{delete, get, post, put};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(public::login))
        .route("/tournaments", get(public::list).post(admin::create))
        .route("/tournaments/{slug}", get(public::show))
        .route("/tournaments/{slug}/events", get(public::events))
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
    Router::new()
        .nest("/api/v1", api)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
