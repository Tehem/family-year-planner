//! Web sessions: encrypted + signed cookie `annee_session` (§11.1), and the request principal.

use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::{HeaderMap, SET_COOKIE};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "annee_session";
const SESSION_DAYS: i64 = 30;
const RENEW_AFTER_SECS: i64 = 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub email: String,
    pub name: String,
    /// Issued at (unix seconds).
    pub iat: i64,
    /// Last renewal (unix seconds).
    pub seen: i64,
}

impl SessionData {
    pub fn new(email: &str, name: &str) -> Self {
        let now = now();
        SessionData {
            email: email.to_string(),
            name: name.to_string(),
            iat: now,
            seen: now,
        }
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Who is calling the web API.
#[derive(Debug, Clone)]
pub enum Principal {
    Member { email: String, name: String },
    Kiosk { token_id: String },
}

impl Principal {
    pub fn is_member(&self) -> bool {
        matches!(self, Principal::Member { .. })
    }

    /// Name recorded in `fy_created_by` / `fy_updated_by`.
    pub fn writer_name(&self) -> &str {
        match self {
            Principal::Member { name, .. } => name,
            Principal::Kiosk { .. } => "Kiosque",
        }
    }
}

pub fn build_cookie(
    state: &AppState,
    name: &'static str,
    value: String,
    max_age_days: i64,
) -> Cookie<'static> {
    Cookie::build((name, value))
        .path("/")
        .http_only(true)
        .secure(state.config.secure_cookies())
        .same_site(SameSite::Lax)
        .max_age(time_days(max_age_days))
        .build()
}

fn time_days(days: i64) -> time::Duration {
    time::Duration::days(days)
}

pub fn session_cookie(state: &AppState, data: &SessionData) -> Cookie<'static> {
    build_cookie(
        state,
        SESSION_COOKIE,
        serde_json::to_string(data).unwrap_or_default(),
        SESSION_DAYS,
    )
}

pub fn clear_cookie(state: &AppState, name: &'static str) -> Cookie<'static> {
    let mut c = build_cookie(state, name, String::new(), 0);
    c.make_removal();
    c
}

/// A valid web session in the request headers, if any.
pub fn read_session(state: &AppState, headers: &HeaderMap) -> Option<SessionData> {
    let jar = PrivateCookieJar::from_headers(headers, state.cookie_key.clone());
    let data: SessionData = serde_json::from_str(jar.get(SESSION_COOKIE)?.value()).ok()?;
    if now() - data.seen > SESSION_DAYS * 86_400 {
        return None;
    }
    // Revoking an email from the allowlist ends its sessions. Demo sessions have no allowlist.
    if !state.is_demo() && !state.config.is_allowed(&data.email) {
        return None;
    }
    Some(data)
}

/// Resolve the principal of a request (web session, or kiosk session once M9 lands).
pub async fn principal_from_headers(state: &AppState, headers: &HeaderMap) -> Option<Principal> {
    if let Some(s) = read_session(state, headers) {
        return Some(Principal::Member {
            email: s.email,
            name: s.name,
        });
    }
    crate::kiosk::kiosk_principal(state, headers).await
}

impl FromRequestParts<AppState> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        principal_from_headers(state, &parts.headers)
            .await
            .ok_or_else(ApiError::unauthorized)
    }
}

/// A principal that may write (web members only).
pub struct Member {
    pub email: String,
    pub name: String,
}

impl FromRequestParts<AppState> for Member {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match Principal::from_request_parts(parts, state).await? {
            Principal::Member { email, name } => Ok(Member { email, name }),
            Principal::Kiosk { .. } => Err(ApiError::read_only_session()),
        }
    }
}

/// Sliding expiry: re-issue the cookie when the session was last renewed over an hour ago.
pub async fn renew_session(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let session = read_session(&state, req.headers());
    let res = next.run(req).await;
    if let Some(mut s) = session
        && now() - s.seen > RENEW_AFTER_SECS
        && !sets_cookie(&res, SESSION_COOKIE)
    {
        s.seen = now();
        let jar = PrivateCookieJar::new(state.cookie_key.clone()).add(session_cookie(&state, &s));
        return (jar, res).into_response();
    }
    res
}

fn sets_cookie(res: &Response, name: &str) -> bool {
    res.headers()
        .get_all(SET_COOKIE)
        .iter()
        .any(|v| v.to_str().is_ok_and(|s| s.starts_with(&format!("{name}="))))
}
