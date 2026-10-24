use axum::body::Body;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::db;
use crate::settings::{self, Appearance};
use crate::state::AppState;
use crate::text::escape_html;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Dist;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const REVALIDATE: &str = "no-cache";

/// Serves the built web app, falling back to index.html for client side routes.
pub async fn serve(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Dist::get(path) {
        Some(file) if path == "index.html" => index(&state, file, path).await,
        Some(file) => respond(path, file, path.starts_with("assets/")),
        None if has_extension(path) => StatusCode::NOT_FOUND.into_response(),
        None => match Dist::get("index.html") {
            Some(file) => index(&state, file, path).await,
            None => (StatusCode::NOT_FOUND, "web app is not built").into_response(),
        },
    }
}

/// index.html carries the saved look (CSS, color scheme, title and a JSON copy for the app), so
/// the very first paint already matches and nothing flips once the settings request returns.
async fn index(state: &AppState, file: rust_embed::EmbeddedFile, path: &str) -> Response {
    let appearance = settings::load(&state.pool).await.unwrap_or_default();
    let preview = link_preview(state, path).await;
    let html = personalize(
        &String::from_utf8_lossy(&file.data),
        &appearance,
        preview.as_ref(),
    );
    Response::builder()
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .header(CACHE_CONTROL, REVALIDATE)
        .body(Body::from(html))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// What a chat shows when a tournament link is pasted: its name and a one line summary.
struct Preview {
    title: String,
    description: String,
}

async fn link_preview(state: &AppState, path: &str) -> Option<Preview> {
    let mut parts = path.split('/');
    let slug = match (parts.next(), parts.next()) {
        (Some("t" | "embed"), Some(slug)) if !slug.is_empty() => slug,
        _ => return None,
    };
    let t = db::get_by_slug(&state.pool, slug).await.ok()?;
    let format = match t.format {
        kymo_engine::Format::SingleElimination => "Single elimination",
        kymo_engine::Format::DoubleElimination => "Double elimination",
        kymo_engine::Format::RoundRobin => "Round robin",
        kymo_engine::Format::GroupKnockout => "Groups then knockout",
        kymo_engine::Format::Swiss => "Swiss",
    };
    let status = match t.status {
        db::Status::Draft => "starting soon".to_owned(),
        db::Status::Live => format!(
            "{} of {} matches played",
            t.progress.played, t.progress.total
        ),
        db::Status::Done => {
            let champion = t
                .champion
                .and_then(|id| t.teams.iter().find(|team| team.id as u32 == id))
                .map(|team| format!(", champion {}", team.name))
                .unwrap_or_default();
            format!("finished{champion}")
        }
    };
    Some(Preview {
        title: t.name.clone(),
        description: format!("{format}, {} teams, {status}", t.team_count),
    })
}

fn personalize(html: &str, a: &Appearance, preview: Option<&Preview>) -> String {
    // A JSON data block is never executed, so it works under a strict script policy.
    let json = serde_json::to_string(a)
        .unwrap_or_default()
        .replace('<', "\\u003c");
    let meta = preview.map_or_else(String::new, |p| {
        let title = escape_html(&format!("{} · {}", p.title, a.site_name));
        let description = escape_html(&p.description);
        format!(
            "<meta property=\"og:type\" content=\"website\"><meta property=\"og:site_name\" content=\"{}\"><meta property=\"og:title\" content=\"{title}\"><meta property=\"og:description\" content=\"{description}\"><meta name=\"description\" content=\"{description}\">",
            escape_html(&a.site_name)
        )
    });
    let head = format!(
        "{meta}<style id=\"kymo-theme\">{}</style><script type=\"application/json\" id=\"kymo-appearance\">{json}</script></head>",
        a.css()
    );
    html.replace("</head>", &head)
        .replace(
            "<html lang=\"en\">",
            &format!("<html lang=\"en\" data-scheme=\"{}\">", a.scheme_attr()),
        )
        .replace(
            "<title>Kymo</title>",
            &format!(
                "<title>{}</title>",
                escape_html(&preview.map_or_else(
                    || a.site_name.clone(),
                    |p| format!("{} · {}", p.title, a.site_name)
                ))
            ),
        )
}

fn respond(path: &str, file: rust_embed::EmbeddedFile, hashed: bool) -> Response {
    let mime = mime_for(path);
    let cache = if hashed { IMMUTABLE } else { REVALIDATE };
    Response::builder()
        .header(CONTENT_TYPE, mime)
        .header(CACHE_CONTROL, cache)
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "png" => "image/png",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// A missing file such as /favicon.ico or /.env should 404, only app routes get index.html.
fn has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
}
