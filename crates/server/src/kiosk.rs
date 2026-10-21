//! Kiosk sessions (§9): a read-only tablet session opened with a kiosk link token.

use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar};

use crate::session::{Principal, build_cookie};
use crate::state::AppState;

pub const KIOSK_COOKIE: &str = "annee_kiosk";
const KIOSK_DAYS: i64 = 400;

/// The cookie that binds a browser to a kiosk token id (revoking the token ends the session).
pub fn kiosk_cookie(state: &AppState, token_id: &str) -> Cookie<'static> {
    build_cookie(state, KIOSK_COOKIE, token_id.to_string(), KIOSK_DAYS)
}

/// A kiosk principal if the request carries a kiosk cookie whose token is still active.
pub async fn kiosk_principal(state: &AppState, headers: &HeaderMap) -> Option<Principal> {
    let jar = PrivateCookieJar::from_headers(headers, state.cookie_key.clone());
    let id = jar.get(KIOSK_COOKIE)?.value().to_string();
    let pat = state.tokens.is_active(&id).await.ok()??;
    pat.kiosk.then_some(Principal::Kiosk { token_id: pat.id })
}
