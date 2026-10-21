//! OAuth 2.1 authorization server for MCP clients (§11.4): discovery, registration, authorization
//! with consent, token endpoint with rotating refresh tokens, revocation.

pub mod authorize;
pub mod clients;
pub mod jwt;
pub mod token;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::RwLock;

use self::clients::{ClientError, Clients, RegistrationRequest};
use self::jwt::{AccessClaims, TokenKeys};
use crate::mcp::McpAuth;
use crate::ratelimit::{request_ip, too_many};
use crate::state::AppState;
use crate::storage::{Db, now_ts, ts};
use crate::tokens::Scope;

pub const ACCESS_TOKEN_SECS: i64 = 3600;
pub const REFRESH_TOKEN_DAYS: i64 = 30;
pub const CODE_MINUTES: i64 = 5;
const REVOKED_CACHE: Duration = Duration::from_secs(30);

pub const SCOPE_READ: &str = "calendar:read";
pub const SCOPE_WRITE: &str = "calendar:write";
pub const SCOPE_OFFLINE: &str = "offline_access";

pub struct OAuthService {
    pub db: Db,
    pub keys: TokenKeys,
    pub clients: Clients,
    pub base_url: String,
    revoked: RwLock<(Option<Instant>, HashSet<String>)>,
}

impl OAuthService {
    pub fn new(db: Db, keys: TokenKeys, base_url: String, allow_insecure_cimd: bool) -> Arc<Self> {
        Arc::new(OAuthService {
            clients: Clients::new(db.clone(), allow_insecure_cimd),
            db,
            keys,
            base_url,
            revoked: RwLock::new((None, HashSet::new())),
        })
    }

    pub fn resource(&self) -> String {
        format!("{}/mcp", self.base_url)
    }

    /// Whether a grant was revoked (in-memory, refreshed at most every 30 seconds).
    pub async fn is_revoked(&self, grant_id: &str) -> bool {
        {
            let g = self.revoked.read().await;
            if g.0.is_some_and(|at| at.elapsed() < REVOKED_CACHE) {
                return g.1.contains(grant_id);
            }
        }
        // Only recent revocations matter: access tokens live one hour.
        let since = ts(Utc::now() - ChronoDuration::hours(2));
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM oauth_grants WHERE revoked_at IS NOT NULL AND revoked_at > ?",
        )
        .bind(since)
        .fetch_all(&self.db.pool)
        .await
        .unwrap_or_default();
        let mut g = self.revoked.write().await;
        *g = (Some(Instant::now()), ids.into_iter().collect());
        g.1.contains(grant_id)
    }

    /// Revoke a grant now (and make this process notice immediately).
    pub async fn revoke_grant(&self, grant_id: &str) -> Result<bool, sqlx::Error> {
        let r = sqlx::query(
            "UPDATE oauth_grants SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(now_ts())
        .bind(grant_id)
        .execute(&self.db.pool)
        .await?;
        self.revoked.write().await.1.insert(grant_id.to_string());
        Ok(r.rows_affected() > 0)
    }

    /// Active grants for Réglages, newest first.
    pub async fn list_grants(&self) -> Result<Vec<Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT g.id, g.scope, g.redirect_host, g.created_at, g.last_used_at, COALESCE(c.client_name, 'Application inconnue') AS client_name \
             FROM oauth_grants g LEFT JOIN oauth_clients c ON c.client_id = g.client_id \
             WHERE g.revoked_at IS NULL ORDER BY g.created_at DESC, g.rowid DESC",
        )
        .fetch_all(&self.db.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                json!({
                    "id": r.get::<String, _>("id"),
                    "client_name": r.get::<String, _>("client_name"),
                    "redirect_host": r.get::<String, _>("redirect_host"),
                    "scope": r.get::<String, _>("scope"),
                    "created_at": r.get::<String, _>("created_at"),
                    "last_used_at": r.get::<Option<String>, _>("last_used_at"),
                })
            })
            .collect())
    }

    /// Delete expired codes and refresh tokens (run daily).
    pub async fn purge_expired(&self) -> Result<(), sqlx::Error> {
        let now = now_ts();
        sqlx::query("DELETE FROM oauth_codes WHERE expires_at < ?")
            .bind(&now)
            .execute(&self.db.pool)
            .await?;
        sqlx::query("DELETE FROM oauth_refresh_tokens WHERE expires_at < ?")
            .bind(&now)
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    async fn touch_grant(&self, grant_id: &str) {
        let cutoff = ts(Utc::now() - ChronoDuration::seconds(60));
        let _ = sqlx::query("UPDATE oauth_grants SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR last_used_at < ?)")
            .bind(now_ts())
            .bind(grant_id)
            .bind(cutoff)
            .execute(&self.db.pool)
            .await;
    }
}

/// Validate an OAuth access token presented on `/mcp`.
pub async fn authenticate_access_token(state: &AppState, token: &str) -> Option<McpAuth> {
    let svc = &state.oauth;
    let claims: AccessClaims = svc.keys.verify(token, &svc.base_url, &svc.resource())?;
    if svc.is_revoked(&claims.grant_id).await {
        return None;
    }
    let scope = if claims.has_scope(SCOPE_WRITE) {
        Scope::ReadWrite
    } else if claims.has_scope(SCOPE_READ) {
        Scope::Read
    } else {
        return None;
    };
    svc.touch_grant(&claims.grant_id).await;
    let name = if claims.client_name.is_empty() {
        "Agent OAuth".to_string()
    } else {
        claims.client_name
    };
    Some(McpAuth {
        name,
        scope,
        key: format!("oauth:{}", claims.grant_id),
    })
}

pub fn router(state: AppState) -> Router<AppState> {
    let limited = Router::new()
        .route(
            "/oauth/register",
            post(register).layer(middleware::from_fn_with_state(
                state.clone(),
                limit_register,
            )),
        )
        .route(
            "/oauth/token",
            post(token::token).layer(middleware::from_fn_with_state(state.clone(), limit_token)),
        )
        .route(
            "/oauth/revoke",
            post(token::revoke).layer(middleware::from_fn_with_state(state.clone(), limit_token)),
        );
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server),
        )
        .route(
            "/oauth/authorize",
            get(authorize::show).post(authorize::decide),
        )
        .merge(limited)
}

async fn limit_register(
    State(s): State<AppState>,
    req: Request,
    next: middleware::Next,
) -> Response {
    if !s.limits.oauth_register.check(&request_ip(&req)) {
        return too_many();
    }
    next.run(req).await
}

async fn limit_token(State(s): State<AppState>, req: Request, next: middleware::Next) -> Response {
    if !s.limits.oauth_token.check(&request_ip(&req)) {
        return too_many();
    }
    next.run(req).await
}

async fn protected_resource(State(s): State<AppState>) -> Json<Value> {
    Json(json!({
        "resource": s.oauth.resource(),
        "authorization_servers": [s.oauth.base_url],
        "scopes_supported": [SCOPE_READ, SCOPE_WRITE],
        "bearer_methods_supported": ["header"],
    }))
}

async fn authorization_server(State(s): State<AppState>) -> Json<Value> {
    let b = &s.oauth.base_url;
    Json(json!({
        "issuer": b,
        "authorization_endpoint": format!("{b}/oauth/authorize"),
        "token_endpoint": format!("{b}/oauth/token"),
        "registration_endpoint": format!("{b}/oauth/register"),
        "revocation_endpoint": format!("{b}/oauth/revoke"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_post", "client_secret_basic"],
        "scopes_supported": [SCOPE_READ, SCOPE_WRITE, SCOPE_OFFLINE],
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
    }))
}

pub fn oauth_error(status: StatusCode, code: &str, description: &str) -> Response {
    let mut res = (
        status,
        Json(json!({"error": code, "error_description": description})),
    )
        .into_response();
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert(header::PRAGMA, "no-cache".parse().unwrap());
    res
}

async fn register(
    State(s): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.trim()
                .to_ascii_lowercase()
                .starts_with("application/json")
        });
    if !is_json {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "Content-Type must be application/json",
        );
    }
    let Ok(req) = serde_json::from_slice::<RegistrationRequest>(&body) else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "invalid JSON body",
        );
    };
    match s.oauth.clients.register(req).await {
        Ok(reg) => {
            let c = &reg.client;
            let mut out = json!({
                "client_id": c.client_id,
                "client_name": c.client_name,
                "redirect_uris": c.redirect_uris,
                "token_endpoint_auth_method": reg.auth_method,
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "client_id_issued_at": Utc::now().timestamp(),
            });
            if let Some(secret) = reg.secret {
                out["client_secret"] = json!(secret);
                out["client_secret_expires_at"] = json!(0);
            }
            let mut res = (StatusCode::CREATED, Json(out)).into_response();
            res.headers_mut()
                .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            res
        }
        Err(ClientError::Invalid(code, msg)) => oauth_error(StatusCode::BAD_REQUEST, code, &msg),
        Err(ClientError::Full) => oauth_error(
            StatusCode::TOO_MANY_REQUESTS,
            "invalid_client_metadata",
            "too many registered clients",
        ),
        Err(ClientError::Unavailable(e)) => {
            tracing::error!(error = %e, "client registration failed");
            oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "registration failed",
            )
        }
    }
}

/// Daily housekeeping: expired OAuth rows and idle rate-limit entries.
pub fn spawn_housekeeping(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        let mut n = 0u32;
        loop {
            tick.tick().await;
            state.limits.cleanup();
            n += 1;
            if n % 24 == 1
                && let Err(e) = state.oauth.purge_expired().await
            {
                tracing::error!(error = %e, "cannot purge expired OAuth rows");
            }
        }
    });
}
