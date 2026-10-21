//! OAuth clients: Dynamic Client Registration (RFC 7591) and Client ID Metadata Documents.

use std::net::IpAddr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use url::Url;

use crate::crypto::{ct_eq, random_token, sha256};
use crate::storage::{Db, now_ts};

pub const MAX_DCR_CLIENTS: i64 = 100;
const CIMD_TTL_HOURS: i64 = 24;
const CIMD_MAX_BYTES: usize = 64 * 1024;
const CIMD_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_NAME_CHARS: usize = 100;
const MAX_REDIRECT_URIS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dcr,
    Cimd,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub client_id: String,
    pub kind: Kind,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    /// Hash of the client secret (confidential DCR clients).
    pub secret_hash: Option<Vec<u8>>,
}

impl Client {
    pub fn is_confidential(&self) -> bool {
        self.secret_hash.is_some()
    }

    pub fn secret_matches(&self, secret: &str) -> bool {
        self.secret_hash
            .as_ref()
            .is_some_and(|h| ct_eq(h, &sha256(secret.as_bytes())))
    }
}

#[derive(Debug)]
pub enum ClientError {
    /// RFC 7591 error code + description.
    Invalid(&'static str, String),
    /// Too many clients registered.
    Full,
    Unavailable(String),
}

fn invalid(code: &'static str, msg: impl Into<String>) -> ClientError {
    ClientError::Invalid(code, msg.into())
}

impl From<sqlx::Error> for ClientError {
    fn from(e: sqlx::Error) -> Self {
        ClientError::Unavailable(e.to_string())
    }
}

pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// `https://…`, or `http://` on a loopback host; no fragment, no credentials.
pub fn valid_redirect_uri(uri: &str) -> bool {
    let Ok(u) = Url::parse(uri) else { return false };
    if u.fragment().is_some() || !u.username().is_empty() || u.password().is_some() {
        return false;
    }
    let Some(host) = u.host_str() else {
        return false;
    };
    match u.scheme() {
        "https" => true,
        "http" => is_loopback_host(host) || is_loopback_host(&format!("[{host}]")),
        _ => false,
    }
}

fn host_of(u: &Url) -> String {
    match u.host() {
        Some(url::Host::Ipv6(ip)) => format!("[{ip}]"),
        Some(h) => h.to_string(),
        None => String::new(),
    }
}

/// Exact match with a registered URI; for loopback URIs the port is ignored (the host, scheme,
/// path and query must still match, so `localhost` does not match `127.0.0.1`).
pub fn redirect_matches(registered: &[String], requested: &str) -> bool {
    if registered.iter().any(|r| r == requested) {
        return true;
    }
    let Ok(req) = Url::parse(requested) else {
        return false;
    };
    if req.scheme() != "http" || !is_loopback_host(&host_of(&req)) {
        return false;
    }
    registered.iter().any(|r| {
        Url::parse(r).is_ok_and(|reg| {
            reg.scheme() == "http"
                && host_of(&reg) == host_of(&req)
                && reg.path() == req.path()
                && reg.query() == req.query()
        })
    })
}

pub fn redirect_host(uri: &str) -> String {
    Url::parse(uri)
        .ok()
        .map(|u| match u.port() {
            Some(p) if !is_loopback_host(&host_of(&u)) => format!("{}:{p}", host_of(&u)),
            _ => host_of(&u),
        })
        .unwrap_or_default()
}

pub fn is_loopback_uri(uri: &str) -> bool {
    Url::parse(uri).is_ok_and(|u| u.scheme() == "http" && is_loopback_host(&host_of(&u)))
}

#[derive(Debug, Deserialize)]
pub struct RegistrationRequest {
    #[serde(default)]
    pub client_name: Option<String>,
    #[serde(default)]
    pub redirect_uris: Option<Vec<String>>,
    #[serde(default)]
    pub token_endpoint_auth_method: Option<String>,
}

pub struct Registered {
    pub client: Client,
    pub secret: Option<String>,
    pub auth_method: &'static str,
}

fn clean_name(name: Option<&str>) -> String {
    let n: String = name
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let n = n.trim();
    if n.is_empty() {
        "Application inconnue".to_string()
    } else {
        n.chars().take(MAX_NAME_CHARS).collect()
    }
}

#[derive(Clone)]
pub struct Clients {
    db: Db,
    /// Tests only: allow `http://` / loopback client metadata URLs.
    allow_insecure_cimd: bool,
}

impl Clients {
    pub fn new(db: Db, allow_insecure_cimd: bool) -> Self {
        Clients {
            db,
            allow_insecure_cimd,
        }
    }

    /// RFC 7591 registration.
    pub async fn register(&self, req: RegistrationRequest) -> Result<Registered, ClientError> {
        let uris = req.redirect_uris.unwrap_or_default();
        if uris.is_empty() {
            return Err(invalid("invalid_redirect_uri", "redirect_uris is required"));
        }
        if uris.len() > MAX_REDIRECT_URIS
            || uris
                .iter()
                .any(|u| !valid_redirect_uri(u) || u.len() > 2000)
        {
            return Err(invalid(
                "invalid_redirect_uri",
                "redirect_uris must be https:// URLs, or http:// on localhost, 127.0.0.1 or [::1]",
            ));
        }
        let (auth_method, confidential) = match req.token_endpoint_auth_method.as_deref() {
            None | Some("none") => ("none", false),
            Some("client_secret_post") => ("client_secret_post", true),
            Some("client_secret_basic") => ("client_secret_basic", true),
            Some(_) => {
                return Err(invalid(
                    "invalid_client_metadata",
                    "unsupported token_endpoint_auth_method",
                ));
            }
        };
        self.make_room().await?;
        let client_id = format!("fy_client_{}", random_token(18));
        let secret = confidential.then(|| format!("fy_secret_{}", random_token(32)));
        let client = Client {
            client_id,
            kind: Kind::Dcr,
            client_name: clean_name(req.client_name.as_deref()),
            redirect_uris: uris,
            secret_hash: secret.as_ref().map(|s| sha256(s.as_bytes())),
        };
        sqlx::query(
            "INSERT INTO oauth_clients (client_id, kind, client_name, redirect_uris, secret_hash, created_at) VALUES (?, 'dcr', ?, ?, ?, ?)",
        )
        .bind(&client.client_id)
        .bind(&client.client_name)
        .bind(json!(client.redirect_uris).to_string())
        .bind(client.secret_hash.as_deref())
        .bind(now_ts())
        .execute(&self.db.pool)
        .await?;
        Ok(Registered {
            client,
            secret,
            auth_method,
        })
    }

    /// At most 100 registered clients: when full, forget unused ones (no active grant) that are
    /// more than a day old, then refuse.
    async fn make_room(&self) -> Result<(), ClientError> {
        let count = || async {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oauth_clients WHERE kind = 'dcr'")
                .fetch_one(&self.db.pool)
                .await
        };
        if count().await? < MAX_DCR_CLIENTS {
            return Ok(());
        }
        let cutoff = crate::storage::ts(Utc::now() - chrono::Duration::days(1));
        sqlx::query(
            "DELETE FROM oauth_clients WHERE kind = 'dcr' AND created_at < ? \
             AND client_id NOT IN (SELECT client_id FROM oauth_grants WHERE revoked_at IS NULL)",
        )
        .bind(cutoff)
        .execute(&self.db.pool)
        .await?;
        if count().await? < MAX_DCR_CLIENTS {
            Ok(())
        } else {
            Err(ClientError::Full)
        }
    }

    /// Look a client up by id; `https://` ids are Client ID Metadata Documents, fetched and
    /// cached for 24 hours.
    pub async fn get(&self, client_id: &str) -> Result<Option<Client>, ClientError> {
        let row = sqlx::query(
            "SELECT client_id, kind, client_name, redirect_uris, secret_hash, metadata_fetched_at FROM oauth_clients WHERE client_id = ?",
        )
        .bind(client_id)
        .fetch_optional(&self.db.pool)
        .await?;
        let stored = row.as_ref().map(|r| {
            let uris: Vec<String> =
                serde_json::from_str(&r.get::<String, _>("redirect_uris")).unwrap_or_default();
            let fetched: Option<String> = r.get("metadata_fetched_at");
            (
                Client {
                    client_id: r.get("client_id"),
                    kind: if r.get::<String, _>("kind") == "cimd" {
                        Kind::Cimd
                    } else {
                        Kind::Dcr
                    },
                    client_name: r.get("client_name"),
                    redirect_uris: uris,
                    secret_hash: r.get("secret_hash"),
                },
                fetched
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
            )
        });
        let is_url = client_id.starts_with("https://")
            || (self.allow_insecure_cimd && client_id.starts_with("http://"));
        if !is_url {
            return Ok(stored.filter(|(c, _)| c.kind == Kind::Dcr).map(|(c, _)| c));
        }
        if let Some((c, Some(at))) = &stored
            && Utc::now() - *at < chrono::Duration::hours(CIMD_TTL_HOURS)
        {
            return Ok(Some(c.clone()));
        }
        match self.fetch_metadata(client_id).await {
            Ok(client) => {
                self.save_cimd(&client).await?;
                Ok(Some(client))
            }
            Err(e) => {
                tracing::warn!(client_id, error = ?e, "client metadata document unavailable");
                // Keep serving a stale copy rather than locking the user out.
                Ok(stored.map(|(c, _)| c))
            }
        }
    }

    async fn save_cimd(&self, c: &Client) -> Result<(), ClientError> {
        sqlx::query(
            "INSERT INTO oauth_clients (client_id, kind, client_name, redirect_uris, created_at, metadata_fetched_at) \
             VALUES (?, 'cimd', ?, ?, ?, ?) \
             ON CONFLICT(client_id) DO UPDATE SET client_name = excluded.client_name, \
             redirect_uris = excluded.redirect_uris, metadata_fetched_at = excluded.metadata_fetched_at",
        )
        .bind(&c.client_id)
        .bind(&c.client_name)
        .bind(json!(c.redirect_uris).to_string())
        .bind(now_ts())
        .bind(now_ts())
        .execute(&self.db.pool)
        .await?;
        Ok(())
    }

    async fn fetch_metadata(&self, url: &str) -> Result<Client, ClientError> {
        let parsed =
            Url::parse(url).map_err(|_| invalid("invalid_client", "invalid client_id URL"))?;
        let insecure_ok = self.allow_insecure_cimd;
        if parsed.scheme() != "https" && !(insecure_ok && parsed.scheme() == "http") {
            return Err(invalid("invalid_client", "client_id must be an https URL"));
        }
        if parsed.fragment().is_some() || !parsed.username().is_empty() {
            return Err(invalid("invalid_client", "invalid client_id URL"));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| invalid("invalid_client", "invalid client_id URL"))?
            .to_string();
        let port = parsed.port_or_known_default().unwrap_or(443);

        // SSRF guard: resolve first, refuse private addresses, then pin the connection to the
        // address we checked (no second DNS lookup).
        let mut builder = reqwest::Client::builder()
            .timeout(CIMD_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("family-year-planner/", env!("CARGO_PKG_VERSION")));
        if !insecure_ok {
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
                .await
                .map_err(|e| ClientError::Unavailable(e.to_string()))?
                .collect();
            if addrs.is_empty() || addrs.iter().any(|a| is_private_ip(a.ip())) {
                return Err(invalid("invalid_client", "client_id host is not allowed"));
            }
            builder = builder.resolve(&host, addrs[0]);
        }
        let http = builder
            .build()
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        let mut resp = http
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Unavailable(format!(
                "metadata answered {}",
                resp.status()
            )));
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?
        {
            body.extend_from_slice(&chunk);
            if body.len() > CIMD_MAX_BYTES {
                return Err(invalid(
                    "invalid_client",
                    "client metadata document is too large",
                ));
            }
        }
        let doc: Value = serde_json::from_slice(&body)
            .map_err(|_| invalid("invalid_client", "invalid metadata JSON"))?;
        if doc.get("client_id").and_then(Value::as_str) != Some(url) {
            return Err(invalid(
                "invalid_client",
                "client_id in the document does not match its URL",
            ));
        }
        if let Some(m) = doc
            .get("token_endpoint_auth_method")
            .and_then(Value::as_str)
            && m != "none"
        {
            return Err(invalid(
                "invalid_client",
                "metadata-document clients must be public",
            ));
        }
        let uris: Vec<String> = doc
            .get("redirect_uris")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if uris.is_empty()
            || uris.len() > MAX_REDIRECT_URIS
            || uris.iter().any(|u| !valid_redirect_uri(u))
        {
            return Err(invalid(
                "invalid_client",
                "invalid redirect_uris in the metadata document",
            ));
        }
        Ok(Client {
            client_id: url.to_string(),
            kind: Kind::Cimd,
            client_name: clean_name(doc.get("client_name").and_then(Value::as_str)),
            redirect_uris: uris,
            secret_hash: None,
        })
    }
}

/// Loopback, private, link-local, CGNAT, unspecified, multicast and ULA addresses.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || (o[0] == 100 && (64..128).contains(&o[1]))
                || o[0] == 0
        }
        IpAddr::V6(v6) => {
            let seg = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00
                || (seg[0] & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|v4| is_private_ip(IpAddr::V4(v4)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(uris: &[&str]) -> Vec<String> {
        uris.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn redirect_uri_syntax() {
        for ok in [
            "https://claude.ai/api/mcp/auth_callback",
            "http://localhost:6274/oauth/callback",
            "http://127.0.0.1/cb",
            "http://[::1]:8000/cb",
        ] {
            assert!(valid_redirect_uri(ok), "{ok}");
        }
        for bad in [
            "http://example.com/cb",
            "ftp://example.com",
            "https://example.com/cb#frag",
            "javascript:alert(1)",
            "/relative",
            "https://user:pw@example.com/cb",
            "",
        ] {
            assert!(!valid_redirect_uri(bad), "{bad}");
        }
    }

    #[test]
    fn exact_match_for_non_loopback() {
        let reg = r(&["https://claude.ai/cb"]);
        assert!(redirect_matches(&reg, "https://claude.ai/cb"));
        assert!(!redirect_matches(&reg, "https://claude.ai/cb2"));
        assert!(!redirect_matches(&reg, "https://claude.ai/cb?x=1"));
        assert!(!redirect_matches(&reg, "https://evil.example/cb"));
        assert!(!redirect_matches(&reg, "http://claude.ai/cb"));
    }

    #[test]
    fn loopback_port_is_ignored_per_host() {
        let reg = r(&["http://localhost:3000/callback"]);
        assert!(redirect_matches(&reg, "http://localhost:54321/callback"));
        assert!(redirect_matches(&reg, "http://localhost/callback"));
        assert!(!redirect_matches(&reg, "http://localhost:3000/other"));
        assert!(
            !redirect_matches(&reg, "http://127.0.0.1:3000/callback"),
            "localhost ≠ 127.0.0.1"
        );
        assert!(!redirect_matches(&reg, "https://localhost:3000/callback"));
        let reg = r(&["http://127.0.0.1:3000/callback"]);
        assert!(redirect_matches(&reg, "http://127.0.0.1:9999/callback"));
        assert!(!redirect_matches(&reg, "http://localhost:3000/callback"));
        let reg = r(&["http://[::1]:3000/callback"]);
        assert!(redirect_matches(&reg, "http://[::1]:7/callback"));
    }

    #[test]
    fn redirect_hosts_for_display() {
        assert_eq!(redirect_host("https://claude.ai/cb"), "claude.ai");
        assert_eq!(
            redirect_host("https://example.com:8443/cb"),
            "example.com:8443"
        );
        assert_eq!(redirect_host("http://localhost:3000/cb"), "localhost");
        assert!(is_loopback_uri("http://127.0.0.1:1/x"));
        assert!(!is_loopback_uri("https://claude.ai/x"));
    }

    #[test]
    fn private_addresses_are_detected() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.1",
            "172.16.5.5",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(is_private_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "93.184.216.34", "2606:4700:4700::1111"] {
            assert!(!is_private_ip(ip.parse().unwrap()), "{ip}");
        }
    }
}
