//! Google Sign-In (OpenID Connect, authorization code flow with PKCE) for household members
//! (§11.1). Google is used for identity only: scopes are `openid email`.

use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Router, middleware};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::crypto::{pkce_challenge, random_token};
use crate::pages::safe_next;
use crate::ratelimit::limit_auth;
use crate::session::{SessionData, build_cookie, clear_cookie, read_session, session_cookie};
use crate::state::AppState;

const FLOW_COOKIE: &str = "annee_oauth";
const FLOW_MINUTES: i64 = 10;
const JWKS_TTL: Duration = Duration::from_secs(3600);
const ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];

/// Google's endpoints (overridable in tests).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth_url: String,
    pub token_url: String,
    pub jwks_url: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            jwks_url: "https://www.googleapis.com/oauth2/v3/certs".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error("code exchange failed: {0}")]
    Exchange(String),
    #[error("invalid ID token: {0}")]
    InvalidToken(String),
}

pub struct GoogleOidc {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    endpoints: Endpoints,
    jwks: RwLock<Option<(Instant, JwkSet)>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Debug, Deserialize)]
struct IdClaims {
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<serde_json::Value>,
    #[serde(default)]
    nonce: Option<String>,
}

/// The verified identity.
#[derive(Debug, PartialEq, Eq)]
pub struct Identity {
    pub email: String,
}

impl GoogleOidc {
    pub fn new(
        http: reqwest::Client,
        client_id: String,
        client_secret: String,
        endpoints: Endpoints,
    ) -> Self {
        GoogleOidc {
            http,
            client_id,
            client_secret,
            endpoints,
            jwks: RwLock::new(None),
        }
    }

    pub fn authorization_url(
        &self,
        redirect_uri: &str,
        state: &str,
        nonce: &str,
        verifier: &str,
    ) -> String {
        let mut url = url::Url::parse(&self.endpoints.auth_url).expect("valid auth url");
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", "openid email")
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", &pkce_challenge(verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("prompt", "select_account");
        url.into()
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        verifier: &str,
    ) -> Result<String, LoginError> {
        let resp = self
            .http
            .post(&self.endpoints.token_url)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .map_err(|e| LoginError::Exchange(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(LoginError::Exchange(format!(
                "token endpoint answered {}",
                resp.status()
            )));
        }
        let t: TokenResponse = resp
            .json()
            .await
            .map_err(|e| LoginError::Exchange(e.to_string()))?;
        Ok(t.id_token)
    }

    async fn fetch_jwks(&self) -> Result<JwkSet, LoginError> {
        let resp = self
            .http
            .get(&self.endpoints.jwks_url)
            .send()
            .await
            .map_err(|e| LoginError::InvalidToken(format!("cannot fetch Google keys: {e}")))?;
        resp.json::<JwkSet>()
            .await
            .map_err(|e| LoginError::InvalidToken(format!("invalid Google keys: {e}")))
    }

    /// Signing key for `kid`, refreshing the cached key set when it is stale or the key is unknown.
    async fn key_for(&self, kid: &str) -> Result<DecodingKey, LoginError> {
        {
            let cache = self.jwks.read().await;
            if let Some((at, set)) = cache.as_ref()
                && at.elapsed() < JWKS_TTL
                && let Some(jwk) = set.find(kid)
            {
                return DecodingKey::from_jwk(jwk)
                    .map_err(|e| LoginError::InvalidToken(e.to_string()));
            }
        }
        let set = self.fetch_jwks().await?;
        let key = set
            .find(kid)
            .ok_or_else(|| LoginError::InvalidToken("unknown signing key".into()))
            .and_then(|jwk| {
                DecodingKey::from_jwk(jwk).map_err(|e| LoginError::InvalidToken(e.to_string()))
            });
        *self.jwks.write().await = Some((Instant::now(), set));
        key
    }

    /// Verify signature, `iss`, `aud`, `exp`, `nonce` and `email_verified`.
    pub async fn verify_id_token(&self, token: &str, nonce: &str) -> Result<Identity, LoginError> {
        let header = decode_header(token).map_err(|e| LoginError::InvalidToken(e.to_string()))?;
        if header.alg != Algorithm::RS256 {
            return Err(LoginError::InvalidToken("unexpected algorithm".into()));
        }
        let kid = header
            .kid
            .ok_or_else(|| LoginError::InvalidToken("missing key id".into()))?;
        let key = self.key_for(&kid).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.client_id]);
        validation.set_issuer(&ISSUERS);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let claims = decode::<IdClaims>(token, &key, &validation)
            .map_err(|e| LoginError::InvalidToken(e.to_string()))?
            .claims;
        if claims.nonce.as_deref() != Some(nonce) {
            return Err(LoginError::InvalidToken("nonce mismatch".into()));
        }
        let verified = match &claims.email_verified {
            Some(serde_json::Value::Bool(b)) => *b,
            Some(serde_json::Value::String(s)) => s == "true",
            _ => false,
        };
        if !verified {
            return Err(LoginError::InvalidToken("email not verified".into()));
        }
        let email = claims
            .email
            .filter(|e| !e.is_empty())
            .ok_or_else(|| LoginError::InvalidToken("no email".into()))?;
        Ok(Identity { email })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Flow {
    state: String,
    nonce: String,
    verifier: String,
    next: String,
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/auth/google/start", get(start))
        .route("/auth/google/callback", get(callback))
        .layer(middleware::from_fn_with_state(
            state.limits.clone(),
            limit_auth,
        ))
}

fn callback_url(state: &AppState) -> String {
    format!("{}/auth/google/callback", state.config.app.base_url)
}

fn login_error(code: &str) -> String {
    format!("/connexion?error={code}")
}

#[derive(Deserialize)]
struct StartQuery {
    next: Option<String>,
}

async fn start(
    State(s): State<AppState>,
    headers: HeaderMap,
    jar: PrivateCookieJar,
    Query(q): Query<StartQuery>,
) -> Response {
    let Some(oidc) = s.oidc.clone() else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    let next = safe_next(q.next.as_deref());
    if read_session(&s, &headers).is_some() {
        return Redirect::to(&next).into_response();
    }
    let flow = Flow {
        state: random_token(24),
        nonce: random_token(24),
        verifier: random_token(32),
        next,
    };
    let url = oidc.authorization_url(&callback_url(&s), &flow.state, &flow.nonce, &flow.verifier);
    let cookie: Cookie<'static> = build_cookie(
        &s,
        FLOW_COOKIE,
        serde_json::to_string(&flow).unwrap_or_default(),
        0,
    )
    .into_owned();
    let mut cookie = cookie;
    cookie.set_max_age(time::Duration::minutes(FLOW_MINUTES));
    (jar.add(cookie), Redirect::to(&url)).into_response()
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(oidc) = s.oidc.clone() else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    let flow: Option<Flow> = jar
        .get(FLOW_COOKIE)
        .and_then(|c| serde_json::from_str(c.value()).ok());
    // The one-shot flow cookie is always consumed.
    let jar = jar.remove(clear_cookie(&s, FLOW_COOKIE));

    let fail =
        |jar: PrivateCookieJar, code: &str| (jar, Redirect::to(&login_error(code))).into_response();
    let Some(flow) = flow else {
        return fail(jar, "invalid");
    };
    if q.state.as_deref() != Some(flow.state.as_str()) {
        return fail(jar, "invalid");
    }
    if let Some(err) = q.error {
        tracing::info!(error = %err, "Google sign-in was not completed");
        return fail(
            jar,
            if err == "access_denied" {
                "denied"
            } else {
                "failed"
            },
        );
    }
    let Some(code) = q.code else {
        return fail(jar, "invalid");
    };

    let result = async {
        let id_token = oidc
            .exchange_code(&code, &callback_url(&s), &flow.verifier)
            .await?;
        oidc.verify_id_token(&id_token, &flow.nonce).await
    }
    .await;
    let identity = match result {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(error = %e, "Google sign-in failed");
            return fail(jar, "failed");
        }
    };
    if !s.config.is_allowed(&identity.email) {
        tracing::info!("Google sign-in refused: email not in the allowlist");
        tracing::debug!(email = %identity.email, "refused email");
        let email: String =
            url::form_urlencoded::byte_serialize(identity.email.as_bytes()).collect();
        return (
            jar,
            Redirect::to(&format!("/connexion?error=not_allowed&email={email}")),
        )
            .into_response();
    }
    let name = s.config.user_name(&identity.email);
    let data = SessionData::new(&identity.email.to_lowercase(), &name);
    tracing::debug!(email = %identity.email, "web login");
    (jar.add(session_cookie(&s, &data)), Redirect::to(&flow.next)).into_response()
}
