//! `/oauth/authorize`: validation, consent page, authorization codes.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::PrivateCookieJar;
use chrono::{Duration, Utc};
use html_escape::{encode_double_quoted_attribute as attr, encode_text as text};

use super::clients::{Client, ClientError, is_loopback_uri, redirect_host, redirect_matches};
use super::{CODE_MINUTES, SCOPE_OFFLINE, SCOPE_READ, SCOPE_WRITE};
use crate::crypto::{ct_eq, random_token, sha256};
use crate::security::origin_ok;
use crate::session::{build_cookie, clear_cookie, read_session};
use crate::state::AppState;
use crate::storage::ts;
use crate::web;

const CONSENT_COOKIE: &str = "annee_consent";

/// A fully validated authorization request.
struct Request {
    client: Client,
    redirect_uri: String,
    challenge: String,
    scope: String,
    state: Option<String>,
}

enum Failure {
    /// Never redirect: the client or redirect URI cannot be trusted.
    Page(StatusCode, &'static str),
    /// Redirect back to the (validated) redirect URI with an OAuth error.
    Redirect {
        to: String,
        error: &'static str,
        description: &'static str,
        state: Option<String>,
    },
}

fn normalize_scope(raw: Option<&str>) -> Option<String> {
    let requested: Vec<&str> = raw
        .unwrap_or("calendar:read calendar:write")
        .split_whitespace()
        .collect();
    if requested
        .iter()
        .any(|s| ![SCOPE_READ, SCOPE_WRITE, SCOPE_OFFLINE].contains(s))
    {
        return None;
    }
    let has = |s: &str| requested.contains(&s);
    if !has(SCOPE_READ) && !has(SCOPE_WRITE) {
        return None;
    }
    let mut out = vec![SCOPE_READ];
    if has(SCOPE_WRITE) {
        out.push(SCOPE_WRITE);
    }
    if has(SCOPE_OFFLINE) {
        out.push(SCOPE_OFFLINE);
    }
    Some(out.join(" "))
}

fn redirect_url(to: &str, params: &[(&str, &str)], iss: &str) -> String {
    let mut url = url::Url::parse(to).expect("validated redirect uri");
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in params {
            q.append_pair(k, v);
        }
        q.append_pair("iss", iss);
    }
    url.into()
}

async fn validate(state: &AppState, p: &HashMap<String, String>) -> Result<Request, Failure> {
    let svc = &state.oauth;
    let Some(client_id) = p.get("client_id") else {
        return Err(Failure::Page(
            StatusCode::BAD_REQUEST,
            "Application inconnue : client_id manquant.",
        ));
    };
    let client = match svc.clients.get(client_id).await {
        Ok(Some(c)) => c,
        Ok(None) | Err(ClientError::Invalid(..)) => {
            return Err(Failure::Page(
                StatusCode::BAD_REQUEST,
                "Cette application n’est pas reconnue.",
            ));
        }
        Err(_) => {
            return Err(Failure::Page(
                StatusCode::BAD_GATEWAY,
                "Impossible de vérifier cette application. Réessayez dans un instant.",
            ));
        }
    };
    let Some(redirect_uri) = p.get("redirect_uri") else {
        return Err(Failure::Page(
            StatusCode::BAD_REQUEST,
            "Adresse de redirection manquante.",
        ));
    };
    if !redirect_matches(&client.redirect_uris, redirect_uri) {
        return Err(Failure::Page(
            StatusCode::BAD_REQUEST,
            "Adresse de redirection non autorisée pour cette application.",
        ));
    }
    let state_param = p.get("state").cloned();
    let fail = |error, description| Failure::Redirect {
        to: redirect_uri.clone(),
        error,
        description,
        state: state_param.clone(),
    };
    if p.get("response_type").map(String::as_str) != Some("code") {
        return Err(fail(
            "unsupported_response_type",
            "response_type must be code",
        ));
    }
    let Some(challenge) = p
        .get("code_challenge")
        .filter(|c| (43..=128).contains(&c.len()))
    else {
        return Err(fail("invalid_request", "a PKCE code_challenge is required"));
    };
    if p.get("code_challenge_method").map(String::as_str) != Some("S256") {
        return Err(fail(
            "invalid_request",
            "code_challenge_method must be S256",
        ));
    }
    if let Some(r) = p.get("resource")
        && *r != svc.resource()
    {
        return Err(fail("invalid_target", "unknown resource"));
    }
    let Some(scope) = normalize_scope(p.get("scope").map(String::as_str)) else {
        return Err(fail("invalid_scope", "unsupported scope"));
    };
    Ok(Request {
        client,
        redirect_uri: redirect_uri.clone(),
        challenge: challenge.clone(),
        scope,
        state: state_param,
    })
}

fn page(status: StatusCode, body_html: &str) -> Response {
    let tpl = web::template("consent.html").unwrap_or_else(|| web::FALLBACK_TEMPLATE.to_string());
    let html = if tpl.contains("<!--BODY-->") {
        tpl.replace("<!--BODY-->", body_html)
    } else {
        tpl
    };
    let mut res = (status, html).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        "text/html; charset=utf-8".parse().unwrap(),
    );
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    res
}

fn error_page(status: StatusCode, message: &str) -> Response {
    page(
        status,
        &format!(
            "<h1>Année en famille</h1><p class=\"lead\">{}</p><p class=\"hint\">Fermez cette page et recommencez depuis l’application.</p>",
            text(message)
        ),
    )
}

fn failure_response(state: &AppState, f: Failure) -> Response {
    match f {
        Failure::Page(status, msg) => error_page(status, msg),
        Failure::Redirect {
            to,
            error,
            description,
            state: st,
        } => {
            let mut params = vec![("error", error), ("error_description", description)];
            if let Some(s) = &st {
                params.push(("state", s));
            }
            Redirect::to(&redirect_url(&to, &params, &state.oauth.base_url)).into_response()
        }
    }
}

fn consent_html(
    req: &Request,
    email: &str,
    original: &HashMap<String, String>,
    csrf: &str,
) -> String {
    let write = req.scope.split(' ').any(|s| s == SCOPE_WRITE);
    let mut hidden = String::new();
    for k in [
        "response_type",
        "client_id",
        "redirect_uri",
        "code_challenge",
        "code_challenge_method",
        "scope",
        "state",
        "resource",
    ] {
        if let Some(v) = original.get(k) {
            hidden.push_str(&format!(
                "<input type=\"hidden\" name=\"{k}\" value=\"{}\">",
                attr(v)
            ));
        }
    }
    hidden.push_str(&format!(
        "<input type=\"hidden\" name=\"csrf\" value=\"{}\">",
        attr(csrf)
    ));
    let warning = if is_loopback_uri(&req.redirect_uri) {
        "<p class=\"warn\">Cette application tourne sur un ordinateur (adresse locale). Ne continuez que si vous venez de la lancer.</p>"
    } else {
        ""
    };
    format!(
        "<h1>Année en famille</h1>\
         <p class=\"lead\"><strong>{name}</strong> demande l’accès à l’agenda familial.</p>\
         <ul class=\"perms\"><li>Voir les événements</li>{write}</ul>\
         <p class=\"redirect\">Vous serez redirigé vers <strong>{host}</strong></p>{warning}\
         <p class=\"who\">Connecté en tant que {email}</p>\
         <form method=\"post\" action=\"/oauth/authorize\">{hidden}\
         <div class=\"dlg-actions\">\
         <button type=\"submit\" name=\"decision\" value=\"deny\" class=\"btn ghost\">Refuser</button>\
         <button type=\"submit\" name=\"decision\" value=\"approve\" class=\"btn\">Autoriser</button>\
         </div></form>",
        name = text(&req.client.client_name),
        write = if write {
            "<li>Ajouter, modifier et supprimer des événements</li>"
        } else {
            ""
        },
        host = text(&redirect_host(&req.redirect_uri)),
        email = text(email),
    )
}

fn login_redirect(original_query: &str) -> Response {
    let target = format!("/oauth/authorize?{original_query}");
    let next: String = url::form_urlencoded::byte_serialize(target.as_bytes()).collect();
    Redirect::to(&format!("/connexion?next={next}")).into_response()
}

pub async fn show(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: PrivateCookieJar,
    uri: axum::http::Uri,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let req = match validate(&state, &params).await {
        Ok(r) => r,
        Err(f) => return failure_response(&state, f),
    };
    let Some(session) = read_session(&state, &headers) else {
        return login_redirect(uri.query().unwrap_or(""));
    };
    let csrf = random_token(24);
    let cookie = build_cookie(&state, CONSENT_COOKIE, csrf.clone(), 0);
    let mut cookie = cookie;
    cookie.set_max_age(time::Duration::minutes(15));
    // The browser enforces the `form-action` of the page that holds the form, and applies it to
    // the redirect that follows the submit, so the consent page itself must not restrict it.
    let mut res = page(
        StatusCode::OK,
        &consent_html(&req, &session.email, &params, &csrf),
    );
    res.headers_mut().insert(
        "content-security-policy",
        axum::http::HeaderValue::from_static(crate::security::CSP_NO_FORM_ACTION),
    );
    (jar.add(cookie), res).into_response()
}

pub async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: PrivateCookieJar,
    body: Bytes,
) -> Response {
    let mut res = decide_inner(state, headers, jar, body).await;
    res.headers_mut().insert(
        "content-security-policy",
        axum::http::HeaderValue::from_static(crate::security::CSP_NO_FORM_ACTION),
    );
    res
}

async fn decide_inner(
    state: AppState,
    headers: HeaderMap,
    jar: PrivateCookieJar,
    body: Bytes,
) -> Response {
    if !origin_ok(&state, &headers) {
        return error_page(StatusCode::FORBIDDEN, "Requête refusée.");
    }
    let mut params: HashMap<String, String> = HashMap::new();
    for (k, v) in url::form_urlencoded::parse(&body) {
        params.insert(k.into_owned(), v.into_owned());
    }
    let Some(session) = read_session(&state, &headers) else {
        return error_page(
            StatusCode::UNAUTHORIZED,
            "Votre session a expiré. Recommencez depuis l’application.",
        );
    };
    let cookie_csrf = jar
        .get(CONSENT_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    let form_csrf = params.get("csrf").cloned().unwrap_or_default();
    let jar = jar.remove(clear_cookie(&state, CONSENT_COOKIE));
    if cookie_csrf.is_empty() || !ct_eq(cookie_csrf.as_bytes(), form_csrf.as_bytes()) {
        return (jar, error_page(StatusCode::FORBIDDEN, "Requête refusée.")).into_response();
    }
    let req = match validate(&state, &params).await {
        Ok(r) => r,
        Err(f) => return (jar, failure_response(&state, f)).into_response(),
    };
    let iss = &state.oauth.base_url;
    let with_state = |mut p: Vec<(&'static str, String)>| {
        if let Some(s) = &req.state {
            p.push(("state", s.clone()));
        }
        p
    };
    if params.get("decision").map(String::as_str) != Some("approve") {
        let p = with_state(vec![
            ("error", "access_denied".into()),
            ("error_description", "the user denied the request".into()),
        ]);
        let refs: Vec<(&str, &str)> = p.iter().map(|(k, v)| (*k, v.as_str())).collect();
        return (
            jar,
            Redirect::to(&redirect_url(&req.redirect_uri, &refs, iss)),
        )
            .into_response();
    }
    let code = random_token(32);
    let expires = ts(Utc::now() + Duration::minutes(CODE_MINUTES));
    let inserted = sqlx::query(
        "INSERT INTO oauth_codes (code_hash, client_id, redirect_uri, code_challenge, scope, email, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(sha256(code.as_bytes()))
    .bind(&req.client.client_id)
    .bind(&req.redirect_uri)
    .bind(&req.challenge)
    .bind(&req.scope)
    .bind(&session.email)
    .bind(expires)
    .execute(&state.db.pool)
    .await;
    if let Err(e) = inserted {
        tracing::error!(error = %e, "cannot store the authorization code");
        return (
            jar,
            error_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Une erreur est survenue.",
            ),
        )
            .into_response();
    }
    let p = with_state(vec![("code", code)]);
    let refs: Vec<(&str, &str)> = p.iter().map(|(k, v)| (*k, v.as_str())).collect();
    (
        jar,
        Redirect::to(&redirect_url(&req.redirect_uri, &refs, iss)),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_normalization() {
        assert_eq!(
            normalize_scope(None).as_deref(),
            Some("calendar:read calendar:write")
        );
        assert_eq!(
            normalize_scope(Some("calendar:read")).as_deref(),
            Some("calendar:read")
        );
        assert_eq!(
            normalize_scope(Some("calendar:write")).as_deref(),
            Some("calendar:read calendar:write")
        );
        assert_eq!(
            normalize_scope(Some("offline_access calendar:read")).as_deref(),
            Some("calendar:read offline_access")
        );
        assert_eq!(normalize_scope(Some("offline_access")), None);
        assert_eq!(normalize_scope(Some("calendar:read admin")), None);
        assert_eq!(normalize_scope(Some("")), None);
    }
}
