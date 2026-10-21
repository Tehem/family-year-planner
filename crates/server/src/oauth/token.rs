//! `/oauth/token` and `/oauth/revoke`.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::Row;

use super::clients::{Client, redirect_host};
use super::jwt::AccessClaims;
use super::{ACCESS_TOKEN_SECS, OAuthService, REFRESH_TOKEN_DAYS, oauth_error};
use crate::crypto::{ct_eq, pkce_challenge, random_token, sha256};
use crate::state::AppState;
use crate::storage::{now_ts, ts};

/// Parse a urlencoded body (rejecting repeated parameters).
fn parse_form(body: &[u8]) -> Option<HashMap<String, String>> {
    let mut out = HashMap::new();
    for (k, v) in url::form_urlencoded::parse(body) {
        if out.insert(k.into_owned(), v.into_owned()).is_some() {
            return None;
        }
    }
    Some(out)
}

fn is_form(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
}

enum ClientAuth {
    Ok(Client),
    Failed(Response),
}

/// Authenticate the client: `none` (public), `client_secret_post` or `client_secret_basic`.
async fn authenticate_client(
    state: &AppState,
    headers: &HeaderMap,
    form: &HashMap<String, String>,
) -> ClientAuth {
    let mut basic: Option<(String, String)> = None;
    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && let Some(b64) = auth
            .strip_prefix("Basic ")
            .or_else(|| auth.strip_prefix("basic "))
        && let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64.trim())
        && let Ok(text) = String::from_utf8(raw)
        && let Some((id, secret)) = text.split_once(':')
    {
        let dec = |s: &str| {
            url::form_urlencoded::parse(s.as_bytes())
                .map(|(k, _)| k.into_owned())
                .next()
                .unwrap_or_default()
        };
        basic = Some((dec(id), dec(secret)));
    }
    let (client_id, secret) = match (&basic, form.get("client_id")) {
        (Some((id, secret)), _) => (id.clone(), Some(secret.clone())),
        (None, Some(id)) => (id.clone(), form.get("client_secret").cloned()),
        (None, None) => {
            return ClientAuth::Failed(oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "client_id is required",
            ));
        }
    };
    let fail = |basic: bool| {
        let mut r = oauth_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
        if basic {
            r.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                "Basic realm=\"oauth\"".parse().unwrap(),
            );
        }
        ClientAuth::Failed(r)
    };
    let client = match state.oauth.clients.get(&client_id).await {
        Ok(Some(c)) => c,
        _ => return fail(basic.is_some()),
    };
    if client.is_confidential() && !secret.as_deref().is_some_and(|s| client.secret_matches(s)) {
        return fail(basic.is_some());
    }
    ClientAuth::Ok(client)
}

fn token_response(body: serde_json::Value) -> Response {
    let mut res = (StatusCode::OK, Json(body)).into_response();
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert(header::PRAGMA, "no-cache".parse().unwrap());
    res
}

fn server_error(e: impl std::fmt::Display) -> Response {
    tracing::error!(error = %e, "oauth token endpoint failure");
    oauth_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "server_error",
        "internal error",
    )
}

impl OAuthService {
    fn issue_access_token(
        &self,
        email: &str,
        client: &Client,
        scope: &str,
        grant_id: &str,
    ) -> Result<String, String> {
        let now = Utc::now().timestamp();
        self.keys.sign(&AccessClaims {
            iss: self.base_url.clone(),
            aud: self.resource(),
            sub: email.to_string(),
            client_id: client.client_id.clone(),
            client_name: client.client_name.clone(),
            scope: scope.to_string(),
            grant_id: grant_id.to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            iat: now,
            exp: now + ACCESS_TOKEN_SECS,
        })
    }

    async fn new_refresh_token(&self, grant_id: &str) -> Result<String, sqlx::Error> {
        let token = random_token(32);
        let now = Utc::now();
        sqlx::query("INSERT INTO oauth_refresh_tokens (token_hash, grant_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
            .bind(sha256(token.as_bytes()))
            .bind(grant_id)
            .bind(ts(now))
            .bind(ts(now + Duration::days(REFRESH_TOKEN_DAYS)))
            .execute(&self.db.pool)
            .await?;
        Ok(token)
    }
}

pub async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if !is_form(&headers) {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Content-Type must be application/x-www-form-urlencoded",
        );
    }
    let Some(form) = parse_form(&body) else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "malformed or repeated parameters",
        );
    };
    let Some(grant_type) = form.get("grant_type") else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "grant_type is required",
        );
    };
    let client = match authenticate_client(&state, &headers, &form).await {
        ClientAuth::Ok(c) => c,
        ClientAuth::Failed(r) => return r,
    };
    if let Some(r) = form.get("resource")
        && *r != state.oauth.resource()
    {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "unknown resource",
        );
    }
    match grant_type.as_str() {
        "authorization_code" => authorization_code(&state, &client, &form).await,
        "refresh_token" => refresh(&state, &client, &form).await,
        _ => oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "only authorization_code and refresh_token are supported",
        ),
    }
}

fn invalid_grant(msg: &str) -> Response {
    oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", msg)
}

async fn authorization_code(
    state: &AppState,
    client: &Client,
    form: &HashMap<String, String>,
) -> Response {
    let svc = &state.oauth;
    let (Some(code), Some(redirect_uri), Some(verifier)) = (
        form.get("code"),
        form.get("redirect_uri"),
        form.get("code_verifier"),
    ) else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "code, redirect_uri and code_verifier are required",
        );
    };
    let row = match sqlx::query(
        "SELECT client_id, redirect_uri, code_challenge, scope, email, expires_at, used_at FROM oauth_codes WHERE code_hash = ?",
    )
    .bind(sha256(code.as_bytes()))
    .fetch_optional(&svc.db.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return server_error(e),
    };
    let Some(row) = row else {
        return invalid_grant("unknown or expired code");
    };
    let stored_client: String = row.get("client_id");
    let stored_redirect: String = row.get("redirect_uri");
    let challenge: String = row.get("code_challenge");
    let scope: String = row.get("scope");
    let email: String = row.get("email");
    let expires_at: String = row.get("expires_at");
    let used_at: Option<String> = row.get("used_at");
    if used_at.is_some() || expires_at < now_ts() {
        return invalid_grant("unknown or expired code");
    }
    if stored_client != client.client_id || stored_redirect != *redirect_uri {
        return invalid_grant("code was not issued for this client or redirect_uri");
    }
    if !(43..=128).contains(&verifier.len())
        || !ct_eq(pkce_challenge(verifier).as_bytes(), challenge.as_bytes())
    {
        return invalid_grant("PKCE verification failed");
    }
    // Single use, atomically.
    match sqlx::query("UPDATE oauth_codes SET used_at = ? WHERE code_hash = ? AND used_at IS NULL")
        .bind(now_ts())
        .bind(sha256(code.as_bytes()))
        .execute(&svc.db.pool)
        .await
    {
        Ok(r) if r.rows_affected() == 1 => {}
        Ok(_) => return invalid_grant("unknown or expired code"),
        Err(e) => return server_error(e),
    }

    let grant_id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = sqlx::query(
        "INSERT INTO oauth_grants (id, client_id, email, scope, redirect_host, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&grant_id)
    .bind(&client.client_id)
    .bind(&email)
    .bind(&scope)
    .bind(redirect_host(redirect_uri))
    .bind(now_ts())
    .execute(&svc.db.pool)
    .await
    {
        return server_error(e);
    }
    issue(svc, client, &email, &scope, &grant_id).await
}

async fn issue(
    svc: &OAuthService,
    client: &Client,
    email: &str,
    scope: &str,
    grant_id: &str,
) -> Response {
    let access = match svc.issue_access_token(email, client, scope, grant_id) {
        Ok(t) => t,
        Err(e) => return server_error(e),
    };
    let refresh = match svc.new_refresh_token(grant_id).await {
        Ok(t) => t,
        Err(e) => return server_error(e),
    };
    token_response(json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": ACCESS_TOKEN_SECS,
        "refresh_token": refresh,
        "scope": scope,
    }))
}

async fn refresh(state: &AppState, client: &Client, form: &HashMap<String, String>) -> Response {
    let svc = &state.oauth;
    let Some(presented) = form.get("refresh_token") else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "refresh_token is required",
        );
    };
    let hash = sha256(presented.as_bytes());
    let row = match sqlx::query(
        "SELECT r.grant_id, r.expires_at, r.rotated_at, g.client_id, g.email, g.scope, g.revoked_at \
         FROM oauth_refresh_tokens r JOIN oauth_grants g ON g.id = r.grant_id WHERE r.token_hash = ?",
    )
    .bind(&hash)
    .fetch_optional(&svc.db.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return server_error(e),
    };
    let Some(row) = row else {
        return invalid_grant("unknown refresh token");
    };
    let grant_id: String = row.get("grant_id");
    let grant_client: String = row.get("client_id");
    if grant_client != client.client_id {
        return invalid_grant("refresh token was not issued for this client");
    }
    let revoked: Option<String> = row.get("revoked_at");
    let rotated: Option<String> = row.get("rotated_at");
    let expires_at: String = row.get("expires_at");
    if revoked.is_some() || expires_at < now_ts() {
        return invalid_grant("refresh token is no longer valid");
    }
    // Rotation: a token can be exchanged once. Presenting a used one means it leaked — revoke the grant.
    let claimed = match sqlx::query("UPDATE oauth_refresh_tokens SET rotated_at = ? WHERE token_hash = ? AND rotated_at IS NULL")
        .bind(now_ts())
        .bind(&hash)
        .execute(&svc.db.pool)
        .await
    {
        Ok(r) => r.rows_affected() == 1,
        Err(e) => return server_error(e),
    };
    if rotated.is_some() || !claimed {
        tracing::warn!(grant_id = %grant_id, "refresh token reuse detected: revoking the grant");
        let _ = svc.revoke_grant(&grant_id).await;
        return invalid_grant("refresh token reuse detected; the authorization was revoked");
    }
    let email: String = row.get("email");
    let granted: String = row.get("scope");
    // A narrower scope may be requested, never a wider one.
    let scope = match form.get("scope") {
        Some(req) => {
            let ok = req
                .split_whitespace()
                .all(|s| granted.split_whitespace().any(|g| g == s));
            if !ok {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_scope",
                    "requested scope exceeds the grant",
                );
            }
            req.clone()
        }
        None => granted,
    };
    issue(svc, client, &email, &scope, &grant_id).await
}

/// RFC 7009: always 200 for well-formed requests, whatever the token.
pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if !is_form(&headers) {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Content-Type must be application/x-www-form-urlencoded",
        );
    }
    let Some(form) = parse_form(&body) else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "malformed or repeated parameters",
        );
    };
    let Some(token) = form.get("token") else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "token is required",
        );
    };
    let client = match authenticate_client(&state, &headers, &form).await {
        ClientAuth::Ok(c) => c,
        ClientAuth::Failed(r) => return r,
    };
    let svc = &state.oauth;
    // A refresh token…
    let grant: Option<String> = sqlx::query_scalar(
        "SELECT r.grant_id FROM oauth_refresh_tokens r JOIN oauth_grants g ON g.id = r.grant_id WHERE r.token_hash = ? AND g.client_id = ?",
    )
    .bind(sha256(token.as_bytes()))
    .bind(&client.client_id)
    .fetch_optional(&svc.db.pool)
    .await
    .ok()
    .flatten();
    // …or an access token.
    let grant = grant.or_else(|| {
        svc.keys
            .verify(token, &svc.base_url, &svc.resource())
            .filter(|c| c.client_id == client.client_id)
            .map(|c| c.grant_id)
    });
    if let Some(g) = grant {
        let _ = svc.revoke_grant(&g).await;
    }
    let mut res = StatusCode::OK.into_response();
    res.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    res
}
