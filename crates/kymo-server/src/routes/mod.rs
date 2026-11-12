pub(crate) mod admin;
mod admin_challenges;
mod challenges;
mod duel;
mod events;
mod instances;
mod labs;
mod monitor;
mod ops;
mod permissions;
mod players;
mod public;
mod scoreboard;
mod site;
mod teams;
mod vpn;

use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Request};
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
        .route("/registration", get(players::registration_status))
        .route("/auth/register", post(players::register))
        .route("/auth/player-login", post(players::player_login))
        .route("/me", get(players::me).put(players::update_me))
        .route("/events", get(events::list))
        .route("/events/{slug}", get(events::show))
        .route(
            "/events/{slug}/register",
            post(events::register).delete(events::unregister),
        )
        .route("/teams", post(teams::create))
        .route("/teams/mine", get(teams::mine))
        .route("/teams/join", post(teams::join))
        .route("/teams/{id}", get(teams::show))
        .route("/teams/{id}/leave", post(teams::leave))
        .route("/teams/{id}/invite-code/rotate", post(teams::rotate_code))
        .route("/teams/{id}/members/{user_id}", delete(teams::kick))
        .route(
            "/admin/events",
            get(events::admin_list).post(events::create),
        )
        .route(
            "/admin/events/{id}",
            put(events::update).delete(events::remove),
        )
        .route("/admin/events/{id}/state", post(events::set_state))
        .route(
            "/admin/events/{id}/registration",
            put(events::set_registration_closed),
        )
        .route(
            "/admin/events/{id}/participants",
            get(events::participants).post(events::add_participant),
        )
        .route(
            "/admin/events/{id}/participants/{participant_id}",
            delete(events::remove_participant),
        )
        .route("/admin/events/{id}/candidates", get(events::candidates))
        .route(
            "/admin/users",
            get(players::list_users).post(players::create_user),
        )
        .route("/admin/users/{id}", put(players::update_user))
        .route(
            "/admin/registration",
            get(players::get_registration).put(players::set_registration),
        )
        .route(
            "/admin/permissions",
            get(permissions::show).put(permissions::update),
        )
        .route("/admin/permissions/reset", post(permissions::reset))
        .route("/permissions/mine", get(permissions::mine))
        .route(
            "/admin/tournaments/{id}/challenge-settings",
            put(duel::save_settings),
        )
        .route(
            "/admin/tournaments/{id}/roster/from-registrations",
            post(duel::fill_roster),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/challenges",
            put(duel::assign),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/duel",
            get(duel::staff_view),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/finalize",
            post(duel::finalize),
        )
        .route(
            "/admin/tournaments/{id}/matches/{match_id}/restart",
            post(duel::restart),
        )
        .route("/tournaments/{slug}/play/mine", get(duel::mine))
        .route(
            "/tournaments/{slug}/matches/{match_id}/play",
            get(duel::play),
        )
        .route(
            "/tournaments/{slug}/matches/{match_id}/challenges/{challenge_id}/submit",
            post(duel::answer),
        )
        .route(
            "/admin/challenges/{id}/items",
            get(admin_challenges::list_items).put(admin_challenges::put_items),
        )
        .route(
            "/admin/contact",
            get(players::get_contact).put(players::set_contact),
        )
        .route("/events/{slug}/challenges", get(challenges::list))
        .route(
            "/events/{slug}/challenges/{id}/submit",
            post(challenges::submit),
        )
        .route(
            "/events/{slug}/challenges/{id}/items/{item_id}/submit",
            post(challenges::submit_item),
        )
        .route(
            "/events/{slug}/challenges/{id}/hints/{hint_id}/unlock",
            post(challenges::unlock_hint),
        )
        .route(
            "/events/{slug}/challenges/{id}/attachments/{attachment_id}",
            get(challenges::download),
        )
        .route(
            "/events/{slug}/challenges/{id}/instance",
            get(instances::show)
                .post(instances::start)
                .delete(instances::stop),
        )
        .route(
            "/events/{slug}/challenges/{id}/instance/restart",
            post(instances::restart),
        )
        .route("/events/{slug}/instances", get(instances::list))
        .route("/admin/events/{id}/instances", get(instances::admin_list))
        .route("/admin/instances/{id}", delete(instances::admin_stop))
        .route("/admin/instances/{id}/flags", get(instances::admin_flags))
        .route(
            "/admin/instances/{id}/verify-log",
            get(instances::admin_verify_log),
        )
        .route("/admin/instances/reap", post(instances::admin_reap))
        .route("/vpn", get(vpn::status))
        .route("/vpn/connection", get(vpn::connection))
        .route("/me/instance", get(instances::mine))
        .route("/vpn/profile", post(vpn::profile))
        .route("/admin/labs", get(labs::list).post(labs::create))
        .route("/admin/labs/{id}", put(labs::update).delete(labs::delete))
        .route("/admin/labs/{id}/test", post(labs::test))
        .route("/admin/labs/{id}/instances", get(labs::instances))
        .route("/admin/labs/{id}/reconcile", post(labs::reconcile))
        .route(
            "/admin/labs/{id}/instances/{name}/screen",
            get(labs::screen),
        )
        .route("/admin/events/{id}/lab", put(labs::set_event_lab))
        .route("/admin/vpn", get(vpn::admin_status))
        .route("/admin/vpn/sync", post(vpn::admin_sync))
        .route("/admin/vpn/clients/{user_id}", delete(vpn::admin_revoke))
        .route("/admin/events/{id}/network", put(vpn::set_network))
        .route("/health", get(ops::live))
        .route("/admin/ops/health", get(ops::health))
        .route("/admin/ops/cleanup", post(ops::cleanup))
        .route(
            "/admin/ops/backups",
            get(ops::list_backups).post(ops::create_backup),
        )
        .route("/admin/ops/backup/download", get(ops::download_backup))
        .route("/admin/events/{id}/export", get(ops::export_event))
        .route(
            "/admin/events/import",
            post(ops::import_event).layer(DefaultBodyLimit::max(ops::BUNDLE_BODY_LIMIT)),
        )
        .route("/admin/monitor/requests", get(monitor::requests))
        .route("/admin/monitor/summary", get(monitor::summary))
        .route(
            "/admin/monitor/events/{id}/submissions",
            get(monitor::submissions),
        )
        .route(
            "/admin/monitor/rules",
            get(monitor::list_rules).post(monitor::create_rule),
        )
        .route(
            "/admin/monitor/rules/{id}",
            put(monitor::update_rule).delete(monitor::delete_rule),
        )
        .route(
            "/admin/monitor/bans",
            get(monitor::list_bans).post(monitor::create_ban),
        )
        .route("/admin/monitor/bans/{id}", delete(monitor::lift_ban))
        .route("/admin/monitor/audit", get(monitor::audit))
        .route("/events/{slug}/scoreboard", get(scoreboard::scoreboard))
        .route(
            "/events/{slug}/scoreboard/timeline",
            get(scoreboard::timeline),
        )
        .route("/events/{slug}/progress", get(scoreboard::progress))
        .route("/events/{slug}/stream", get(challenges::stream))
        .route("/teams/{id}/progress", get(scoreboard::team_progress))
        .route(
            "/admin/events/{id}/categories",
            get(admin_challenges::list_categories).post(admin_challenges::create_category),
        )
        .route(
            "/admin/categories/{id}",
            put(admin_challenges::update_category).delete(admin_challenges::delete_category),
        )
        .route(
            "/admin/events/{id}/challenges",
            get(admin_challenges::list_challenges).post(admin_challenges::create_challenge),
        )
        .route(
            "/admin/challenges/{id}",
            get(admin_challenges::show_challenge)
                .put(admin_challenges::update_challenge)
                .delete(admin_challenges::delete_challenge),
        )
        .route(
            "/admin/challenges/{id}/publish",
            post(admin_challenges::publish),
        )
        .route(
            "/admin/challenges/{id}/retire",
            post(admin_challenges::retire),
        )
        .route(
            "/admin/challenges/{id}/schedule",
            put(admin_challenges::schedule),
        )
        .route(
            "/admin/challenges/{id}/test",
            post(admin_challenges::test_flag),
        )
        .route(
            "/admin/challenges/{id}/hints",
            post(admin_challenges::create_hint),
        )
        .route(
            "/admin/hints/{id}",
            put(admin_challenges::update_hint).delete(admin_challenges::delete_hint),
        )
        .route(
            "/admin/challenges/{id}/attachments",
            post(admin_challenges::upload_attachment)
                .layer(DefaultBodyLimit::max(admin_challenges::UPLOAD_BODY_LIMIT)),
        )
        .route(
            "/admin/attachments/{id}",
            delete(admin_challenges::delete_attachment),
        )
        .route(
            "/admin/events/{id}/submissions",
            get(admin_challenges::submissions),
        )
        .route(
            "/admin/events/{id}/submissions/{sid}/value",
            get(admin_challenges::reveal_submission),
        )
        .route(
            "/admin/events/{id}/jeopardy",
            get(admin_challenges::get_settings).put(admin_challenges::put_settings),
        )
        .route(
            "/admin/events/{id}/scoreboard",
            post(admin_challenges::set_scoreboard),
        )
        .route("/ladder", get(public::ladder))
        .route("/ladder/timeline", get(public::ladder_timeline))
        .route(
            "/admin/events/{id}/scoreboard.csv",
            get(scoreboard::export_csv),
        )
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
        .route("/home", get(site::home))
        .route("/admin/home", put(site::save_home).delete(site::reset_home))
        .route("/public/events/{slug}", get(site::public_event))
        .route("/public/scoreboards", get(site::public_scoreboards))
        .route("/events/{slug}/rules", get(site::rules))
        .route("/admin/events/{id}/rules", put(site::save_rules))
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
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::gate::require_login,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::monitor::layer,
                ))
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

const CSP_BASE: &str =
    "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:";

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
