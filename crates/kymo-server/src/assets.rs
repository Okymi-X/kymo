use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Dist;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const REVALIDATE: &str = "no-cache";

/// Serves the built web app, falling back to index.html for client side routes.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Dist::get(path) {
        Some(file) => respond(path, file, path.starts_with("assets/")),
        None if has_extension(path) => StatusCode::NOT_FOUND.into_response(),
        None => match Dist::get("index.html") {
            Some(index) => respond("index.html", index, false),
            None => (StatusCode::NOT_FOUND, "web app is not built").into_response(),
        },
    }
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
