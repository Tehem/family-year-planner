//! Rate limits (§15.2): per client IP for `/auth/*`, `/oauth/*`, and per token for `/mcp`.

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};

use crate::error::ApiError;

/// A keyed limiter ("n requests per window per key").
pub struct KeyedLimiter {
    inner: DefaultKeyedRateLimiter<String>,
}

impl KeyedLimiter {
    pub fn per_minute(n: u32) -> Self {
        let q = Quota::per_minute(NonZeroU32::new(n).expect("non-zero"));
        KeyedLimiter {
            inner: RateLimiter::keyed(q),
        }
    }

    pub fn per_hour(n: u32) -> Self {
        let q = Quota::per_hour(NonZeroU32::new(n).expect("non-zero"));
        KeyedLimiter {
            inner: RateLimiter::keyed(q),
        }
    }

    /// `true` if the request is allowed.
    pub fn check(&self, key: &str) -> bool {
        self.inner.check_key(&key.to_string()).is_ok()
    }

    /// Drop state of keys that have fully recovered.
    pub fn cleanup(&self) {
        self.inner.retain_recent();
        self.inner.shrink_to_fit();
    }
}

/// All limiters of the server.
pub struct Limits {
    pub auth: KeyedLimiter,
    pub oauth_register: KeyedLimiter,
    pub oauth_token: KeyedLimiter,
    pub mcp: KeyedLimiter,
}

impl Limits {
    pub fn standard() -> Arc<Limits> {
        Arc::new(Limits {
            auth: KeyedLimiter::per_minute(30),
            oauth_register: KeyedLimiter::per_hour(10),
            oauth_token: KeyedLimiter::per_minute(60),
            mcp: KeyedLimiter::per_minute(120),
        })
    }

    pub fn cleanup(&self) {
        self.auth.cleanup();
        self.oauth_register.cleanup();
        self.oauth_token.cleanup();
        self.mcp.cleanup();
    }
}

/// Client address: the first `X-Forwarded-For` entry (set by Caddy, which is the only way to
/// reach the app in the Compose deployment), else the socket address.
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    if let Some(first) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| s.parse::<IpAddr>().is_ok())
    {
        return first.to_string();
    }
    peer.map(|p| p.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn too_many() -> Response {
    let mut res = ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        crate::i18n::tr(
            "Trop de requêtes. Réessayez dans un instant.",
            "Too many requests. Try again in a moment.",
        ),
    )
    .into_response();
    res.headers_mut()
        .insert(header::RETRY_AFTER, header::HeaderValue::from_static("60"));
    res
}

pub fn request_ip(req: &Request) -> String {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    client_ip(req.headers(), peer)
}

/// Layer for `/auth/*`: 30 requests per minute per IP.
pub async fn limit_auth(State(limits): State<Arc<Limits>>, req: Request, next: Next) -> Response {
    if !limits.auth.check(&request_ip(&req)) {
        return too_many();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_allows_the_quota_then_blocks() {
        let l = KeyedLimiter::per_minute(3);
        assert!((0..3).all(|_| l.check("a")));
        assert!(!l.check("a"));
        assert!(l.check("b"), "keys are independent");
    }

    #[test]
    fn client_ip_prefers_forwarded_for() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        assert_eq!(
            client_ip(&h, Some("127.0.0.1:1".parse().unwrap())),
            "203.0.113.7"
        );
        h.insert("x-forwarded-for", "garbage".parse().unwrap());
        assert_eq!(
            client_ip(&h, Some("127.0.0.1:1".parse().unwrap())),
            "127.0.0.1"
        );
        assert_eq!(client_ip(&HeaderMap::new(), None), "unknown");
    }
}
