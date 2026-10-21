//! Security headers (§15.2) and the CSRF guard.

use axum::extract::{Request, State};
use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::state::AppState;

pub const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self'; \
script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

/// The same policy without `form-action`: Chromium also applies `form-action` to the redirect that
/// answers a form post, which would block the consent decision redirecting to the client's own
/// (other-origin) redirect URI. Used only for the `POST /oauth/authorize` response.
pub const CSP_NO_FORM_ACTION: &str = "default-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self'; \
script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'";

/// Add the security headers to every response.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    // A handler may set its own CSP (the consent decision, see `CSP_NO_FORM_ACTION`).
    if !h.contains_key("content-security-policy") {
        h.insert(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(CSP),
        );
    }
    h.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("same-origin"),
    );
    res
}

fn is_mutating(m: &Method) -> bool {
    !matches!(*m, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// `Origin`, when present, must be this server's own origin (the `Host` header, or the origin of
/// `base_url`).
pub fn origin_ok(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    if origin == state.config.origin() {
        return true;
    }
    let host = headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let origin_host = origin.split_once("://").map(|(_, h)| h).unwrap_or(origin);
    !host.is_empty() && origin_host == host
}

fn csrf_error() -> Response {
    ApiError::forbidden("Requête refusée.").into_response()
}

/// Guard for `/api`: mutating requests need `X-Requested-With: annee` and a same-origin `Origin`.
pub async fn csrf_guard_api(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if is_mutating(req.method()) {
        let header_ok = req
            .headers()
            .get("x-requested-with")
            .is_some_and(|v| v == "annee");
        if !header_ok || !origin_ok(&state, req.headers()) {
            return csrf_error();
        }
    }
    next.run(req).await
}

/// Guard for form posts under `/auth`: same-origin `Origin` only (SameSite=Lax covers the rest).
pub async fn csrf_guard_forms(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if is_mutating(req.method()) && !origin_ok(&state, req.headers()) {
        return csrf_error();
    }
    next.run(req).await
}
