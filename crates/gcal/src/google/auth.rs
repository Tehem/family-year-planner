//! Service-account authentication (OAuth 2.0 JWT bearer flow).

use std::time::{Duration, Instant};

use async_trait::async_trait;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::store::StoreError;

pub const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

#[async_trait]
pub trait TokenProvider: Send + Sync {
    async fn access_token(&self) -> Result<String, StoreError>;
}

/// A fixed token (tests).
pub struct StaticToken(pub String);

#[async_trait]
impl TokenProvider for StaticToken {
    async fn access_token(&self) -> Result<String, StoreError> {
        Ok(self.0.clone())
    }
}

/// The relevant fields of a service-account JSON key.
#[derive(Clone, Deserialize)]
pub struct ServiceAccountKey {
    pub client_email: String,
    pub private_key: String,
    #[serde(default = "default_token_uri")]
    pub token_uri: String,
}

fn default_token_uri() -> String {
    DEFAULT_TOKEN_URI.to_string()
}

impl std::fmt::Debug for ServiceAccountKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceAccountKey")
            .field("client_email", &self.client_email)
            .finish_non_exhaustive()
    }
}

impl ServiceAccountKey {
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("invalid service account key: {e}"))
    }

    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::from_json(&json)
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default = "default_expiry")]
    expires_in: u64,
}

fn default_expiry() -> u64 {
    3600
}

pub struct ServiceAccountAuth {
    key: ServiceAccountKey,
    encoding: EncodingKey,
    http: reqwest::Client,
    cache: Mutex<Option<(String, Instant)>>,
}

impl ServiceAccountAuth {
    pub fn new(key: ServiceAccountKey, http: reqwest::Client) -> Result<Self, String> {
        let encoding = EncodingKey::from_rsa_pem(key.private_key.as_bytes())
            .map_err(|e| format!("invalid service account private key: {e}"))?;
        Ok(ServiceAccountAuth {
            key,
            encoding,
            http,
            cache: Mutex::new(None),
        })
    }

    fn assertion(&self) -> Result<String, StoreError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let claims = Claims {
            iss: &self.key.client_email,
            scope: CALENDAR_SCOPE,
            aud: &self.key.token_uri,
            iat: now,
            exp: now + 3600,
        };
        jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &self.encoding).map_err(|e| {
            StoreError::Upstream(format!("cannot sign service account assertion: {e}"))
        })
    }
}

#[async_trait]
impl TokenProvider for ServiceAccountAuth {
    async fn access_token(&self) -> Result<String, StoreError> {
        let mut cache = self.cache.lock().await;
        if let Some((token, valid_until)) = cache.as_ref()
            && Instant::now() < *valid_until
        {
            return Ok(token.clone());
        }
        let assertion = self.assertion()?;
        let resp = self
            .http
            .post(&self.key.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
            ])
            .send()
            .await
            .map_err(|e| StoreError::Upstream(format!("token request failed: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(StoreError::Upstream(format!(
                "token endpoint answered {status}: {}",
                truncate(&body)
            )));
        }
        let t: TokenResponse = resp
            .json()
            .await
            .map_err(|e| StoreError::Upstream(format!("invalid token response: {e}")))?;
        let valid_until =
            Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(120).max(30));
        *cache = Some((t.access_token.clone(), valid_until));
        Ok(t.access_token)
    }
}

pub(crate) fn truncate(s: &str) -> String {
    s.chars().take(300).collect()
}
