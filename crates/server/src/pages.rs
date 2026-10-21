//! HTML pages and the login/logout routes.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Router, middleware};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::Deserialize;

use crate::kiosk::kiosk_cookie;
use crate::ratelimit::limit_auth;
use crate::security::csrf_guard_forms;
use crate::session::{
    SESSION_COOKIE, SessionData, clear_cookie, principal_from_headers, read_session, session_cookie,
};
use crate::state::AppState;
use crate::web;

/// Only relative paths are accepted as `next` (no open redirect).
pub fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n)
            if n.starts_with('/')
                && !n.starts_with("//")
                && !n.contains('\\')
                && !n.contains('\n') =>
        {
            n.to_string()
        }
        _ => "/".to_string(),
    }
}

#[derive(Deserialize)]
struct NextQuery {
    next: Option<String>,
}

pub fn router(state: AppState) -> Router<AppState> {
    let forms = Router::new()
        .route("/auth/demo", post(demo_login))
        .route("/auth/logout", post(logout))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            csrf_guard_forms,
        ))
        .layer(middleware::from_fn_with_state(
            state.limits.clone(),
            limit_auth,
        ));
    Router::new()
        .route("/", get(index))
        .route("/connexion", get(connexion))
        .route("/reglages", get(reglages))
        .route("/kiosk", get(kiosk))
        .merge(forms)
}

fn login_redirect(path: &str) -> Response {
    let next: String = path
        .bytes()
        .map(|b| {
            if b == b'/' {
                "%2F".to_string()
            } else {
                (b as char).to_string()
            }
        })
        .collect();
    Redirect::to(&format!("/connexion?next={next}")).into_response()
}

fn no_store(mut res: Response) -> Response {
    res.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    res
}

async fn index(State(s): State<AppState>, headers: HeaderMap) -> Response {
    // The main screen needs a web session (a kiosk session is for /kiosk only).
    match principal_from_headers(&s, &headers).await {
        Some(p) if p.is_member() => no_store(web::serve("index.html")),
        _ => login_redirect("/"),
    }
}

async fn reglages(State(s): State<AppState>, headers: HeaderMap) -> Response {
    match principal_from_headers(&s, &headers).await {
        Some(p) if p.is_member() => no_store(web::serve("reglages.html")),
        _ => login_redirect("/reglages"),
    }
}

async fn connexion(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<NextQuery>,
) -> Response {
    if read_session(&s, &headers).is_some() {
        return Redirect::to(&safe_next(q.next.as_deref())).into_response();
    }
    no_store(web::serve("connexion.html"))
}

#[derive(Deserialize)]
struct KioskQuery {
    token: Option<String>,
}

/// `/kiosk?token=fy_pat_…` exchanges a kiosk link for a long-lived cookie and removes the token
/// from the URL; `/kiosk` itself needs a kiosk or web session.
async fn kiosk(
    State(s): State<AppState>,
    headers: HeaderMap,
    jar: PrivateCookieJar,
    Query(q): Query<KioskQuery>,
) -> Response {
    if let Some(token) = q.token {
        return match s.tokens.authenticate(&token).await {
            Ok(Some(pat)) if pat.kiosk => {
                (jar.add(kiosk_cookie(&s, &pat.id)), Redirect::to("/kiosk")).into_response()
            }
            _ => Redirect::to("/connexion?error=kiosk").into_response(),
        };
    }
    match principal_from_headers(&s, &headers).await {
        Some(_) => no_store(web::serve("kiosk.html")),
        None => login_redirect("/kiosk"),
    }
}

/// Demo mode only: sign in as "Alex" without Google.
async fn demo_login(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<NextQuery>,
) -> Response {
    if !s.is_demo() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let data = SessionData::new("alex@demo.local", "Alex");
    let jar = jar.add(session_cookie(&s, &data));
    (jar, Redirect::to(&safe_next(q.next.as_deref()))).into_response()
}

async fn logout(State(s): State<AppState>, jar: PrivateCookieJar) -> Response {
    let jar = jar.remove(clear_cookie(&s, SESSION_COOKIE));
    (jar, Redirect::to("/connexion")).into_response()
}
