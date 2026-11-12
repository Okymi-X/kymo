//! Who may use the API without signing in. Everything needs a valid session except:
//!
//! * signing in and signing up, and the appearance and home page content the sign in page shows;
//! * a team's score report, whose code is its own credential;
//! * the read only views of an event whose scoreboard is public: its scoreboard, a short summary,
//!   and for a tournament its bracket data (what the embed, stage and calendar pages read).
//!
//! The rule lives here, in one place, so no handler has to remember it and a new route is closed
//! by default. Answers for anonymous visitors are kept for a few seconds, so a crowd watching a
//! public scoreboard costs the server one computation per event every few seconds.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::error::ApiError;
use crate::state::AppState;
use crate::tokens;

/// How long an answer to an anonymous visitor is reused, in seconds.
const PUBLIC_TTL_SECS: i64 = 5;
/// Most answers kept at once; the cache is emptied when it is full, so it never grows.
const PUBLIC_CACHE_MAX: usize = 256;
/// Largest answer that is kept.
const PUBLIC_BODY_MAX: usize = 2 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum Rule {
    /// Anyone, whatever they send.
    Open,
    /// Anyone, if the event with this slug has a public scoreboard. A session also passes.
    PublicEvent(String),
    /// The same for a tournament slug.
    PublicTournament(String),
    /// A valid session.
    Login,
}

fn rule(method: &Method, path: &str) -> Rule {
    let path = path.strip_prefix("/api/v1").unwrap_or(path);
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let get = method == Method::GET || method == Method::HEAD;
    match (parts.as_slice(), get) {
        (["auth", "login" | "logout" | "register" | "player-login"], _)
            if method == Method::POST =>
        {
            Rule::Open
        }
        (["auth", "me"] | ["registration"] | ["settings"] | ["home"], true)
        | (["public", "scoreboards"], true)
        // Load balancer probes, which only get "ok" without a staff token; and the list of what a
        // visitor may do, which the web app asks for before anyone has signed in.
        | (["health"], true)
        | (["permissions", "mine"], true) => Rule::Open,
        (["report", _], _) => Rule::Open,
        // The live update streams send only a "something changed" ping, no data. A browser's
        // EventSource cannot send the session header, so these cannot ask for a login.
        (["events", _, "stream"] | ["tournaments", _, "events"], true) => Rule::Open,
        (["events", slug, "scoreboard"] | ["events", slug, "scoreboard", "timeline"], true) => {
            Rule::PublicEvent((*slug).to_owned())
        }
        (["public", "events", slug], true) => Rule::PublicEvent((*slug).to_owned()),
        // A player's own view of a match and the duel endpoints are theirs, not the public's.
        (["tournaments", _, "play" | "matches", ..], _) => Rule::Login,
        (["tournaments", slug, ..], true) => Rule::PublicTournament((*slug).to_owned()),
        _ => Rule::Login,
    }
}

async fn event_is_public(state: &AppState, slug: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT scoreboard_public FROM event WHERE slug = ? AND state != 'draft'",
    )
    .bind(slug)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

async fn tournament_is_public(state: &AppState, slug: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT e.scoreboard_public FROM tournament t JOIN event e ON e.id = t.event_id WHERE t.slug = ? AND e.state != 'draft'",
    )
    .bind(slug)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

/// The session secret a request sends, if any.
fn secret_of(request: &Request) -> Option<String> {
    request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
}

pub async fn require_login(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let ruling = rule(request.method(), request.uri().path());
    if ruling == Rule::Open {
        return next.run(request).await;
    }
    // The request is not held across an await: its body is not Sync.
    let secret = secret_of(&request);
    let signed_in = match &secret {
        Some(secret) => matches!(tokens::access(&state.pool, secret).await, Ok(Some(_))),
        None => false,
    };
    if signed_in {
        return next.run(request).await;
    }
    // No valid session. A stale token is refused even on a public view, so an expired session
    // shows as signed out instead of quietly working.
    let sent_token = request.headers().contains_key(AUTHORIZATION);
    let public = !sent_token
        && match &ruling {
            Rule::PublicEvent(slug) => event_is_public(&state, slug).await,
            Rule::PublicTournament(slug) => tournament_is_public(&state, slug).await,
            Rule::Open | Rule::Login => false,
        };
    if !public {
        return ApiError::Unauthorized.into_response();
    }
    answer_from_cache(&state, request, next).await
}

/// Serves an anonymous visitor of a public view from the last answer when it is fresh.
async fn answer_from_cache(state: &AppState, request: Request, next: Next) -> Response {
    let key = request
        .uri()
        .path_and_query()
        .map_or_else(|| request.uri().path().to_owned(), ToString::to_string);
    let now = state.now();
    if let Some((at, body, content_type)) = state
        .public_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned()
        && now - at < PUBLIC_TTL_SECS
    {
        let mut response = Response::new(Body::from(body));
        if let Ok(value) = HeaderValue::from_str(&content_type) {
            response.headers_mut().insert(CONTENT_TYPE, value);
        }
        return response;
    }
    let response = next.run(request).await;
    if response.status() != StatusCode::OK {
        return response;
    }
    let (parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, PUBLIC_BODY_MAX).await else {
        return ApiError::Internal.into_response();
    };
    let content_type = parts
        .headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_owned();
    {
        let mut cache = state
            .public_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if cache.len() >= PUBLIC_CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, (now, Bytes::clone(&bytes), content_type));
    }
    Response::from_parts(parts, Body::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_of(method: Method, path: &str) -> Rule {
        rule(&method, path)
    }

    #[test]
    fn only_sign_in_content_and_public_views_are_open() {
        assert_eq!(rule_of(Method::POST, "/auth/login"), Rule::Open);
        assert_eq!(rule_of(Method::POST, "/auth/register"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/settings"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/home"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/public/scoreboards"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/health"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/permissions/mine"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/registration"), Rule::Open);
        assert_eq!(rule_of(Method::POST, "/report/abc"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/events/cup/stream"), Rule::Open);
        assert_eq!(rule_of(Method::GET, "/tournaments/cup/events"), Rule::Open);
        // Reading the sign in pages is open; writing to them is not.
        assert_eq!(rule_of(Method::PUT, "/settings"), Rule::Login);
        assert_eq!(rule_of(Method::GET, "/auth/login"), Rule::Login);
    }

    #[test]
    fn everything_else_needs_a_session_by_default() {
        for path in [
            "/events",
            "/events/x",
            "/events/x/challenges",
            "/tournaments",
            "/ladder",
            "/me",
            "/vpn",
            "/admin/events",
            "/some/new/route",
        ] {
            assert_eq!(rule_of(Method::GET, path), Rule::Login, "{path}");
        }
        assert_eq!(rule_of(Method::POST, "/tournaments"), Rule::Login);
    }

    #[test]
    fn public_views_are_reads_of_one_event_or_tournament() {
        assert_eq!(
            rule_of(Method::GET, "/events/cup/scoreboard"),
            Rule::PublicEvent("cup".into())
        );
        assert_eq!(
            rule_of(Method::GET, "/events/cup/scoreboard/timeline"),
            Rule::PublicEvent("cup".into())
        );
        assert_eq!(
            rule_of(Method::GET, "/public/events/cup"),
            Rule::PublicEvent("cup".into())
        );
        assert_eq!(
            rule_of(Method::GET, "/tournaments/cup"),
            Rule::PublicTournament("cup".into())
        );
        assert_eq!(
            rule_of(Method::GET, "/tournaments/cup/calendar.ics"),
            Rule::PublicTournament("cup".into())
        );
        // Writes and a player's own views are never public.
        assert_eq!(rule_of(Method::POST, "/events/cup/scoreboard"), Rule::Login);
        assert_eq!(
            rule_of(Method::GET, "/tournaments/cup/play/mine"),
            Rule::Login
        );
        assert_eq!(
            rule_of(Method::GET, "/tournaments/cup/matches/3/play"),
            Rule::Login
        );
        // The prefix, when the router leaves it on, makes no difference.
        assert_eq!(rule_of(Method::GET, "/api/v1/home"), Rule::Open);
    }
}
