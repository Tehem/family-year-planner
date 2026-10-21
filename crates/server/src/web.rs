//! Embedded static web app (Vite build output).

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Dist;

pub fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

const PLACEHOLDER: &str = "<!doctype html><html lang=\"fr\"><meta charset=\"utf-8\">\
<title>Année en famille</title><body><h1>Année en famille</h1>\
<p>L’interface n’est pas encore construite (npm --prefix web run build).</p></body></html>";

/// Serve an embedded file by its path in `web/dist` (e.g. `index.html`, `assets/app-abc.js`).
pub fn serve(path: &str) -> Response {
    let Some(file) = Dist::get(path) else {
        if path.ends_with(".html") {
            return html(PLACEHOLDER.to_string());
        }
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut res = Response::new(Body::from(file.data.into_owned()));
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref())
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    res
}

fn html(body: String) -> Response {
    let mut res = Response::new(Body::from(body));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    res
}

/// An embedded HTML template as text (server-rendered pages such as the OAuth consent screen).
pub fn template(path: &str) -> Option<String> {
    Dist::get(path).and_then(|f| String::from_utf8(f.data.into_owned()).ok())
}

/// Fallback when the front-end bundle has not been built.
pub const FALLBACK_TEMPLATE: &str = "<!doctype html><html lang=\"<!--LANG-->\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title><!--TITLE--></title></head>\
<body><main><!--BODY--></main></body></html>";
