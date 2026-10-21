//! OAuth 2.1 authorization server (§11.4): discovery, DCR, CIMD, PKCE, tokens, rotation, MCP access.

mod common;

use std::collections::HashMap;

use annee_server::crypto::pkce_challenge;
use annee_server::oauth::jwt::AccessClaims;
use annee_server::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum_extra::extract::cookie::Key;
use base64::Engine;
use common::{Reply, TestApp, spawn_server};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CB: &str = "https://claude.ai/api/mcp/auth_callback";

async fn app() -> TestApp {
    let state = AppState::demo_with(
        annee_server::config::AppConfig::default(),
        chrono::NaiveDate::from_ymd_opt(2026, 10, 16),
        Key::generate(),
        true,
    )
    .await
    .unwrap();
    TestApp::with_state(state)
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

async fn post_form(
    app: &mut TestApp,
    uri: &str,
    pairs: &[(&str, &str)],
    extra: &[(&str, &str)],
) -> Reply {
    let mut b =
        Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    for (k, v) in extra {
        b = b.header(*k, *v);
    }
    app.send(b.body(Body::from(form(pairs))).unwrap()).await
}

async fn register(app: &mut TestApp, body: Value) -> Reply {
    app.send(
        Request::post("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn public_client(app: &mut TestApp, name: &str, redirect: &str) -> String {
    let r = register(
        app,
        json!({"client_name": name, "redirect_uris": [redirect]}),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    r.json()["client_id"].as_str().unwrap().to_string()
}

fn authorize_query(client_id: &str, redirect: &str, scope: Option<&str>, state: &str) -> String {
    let challenge = pkce_challenge(VERIFIER);
    let mut pairs = vec![
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state),
    ];
    if let Some(s) = scope {
        pairs.push(("scope", s));
    }
    form(&pairs)
}

fn location_params(r: &Reply) -> (String, HashMap<String, String>) {
    let loc = r
        .header("location")
        .unwrap_or_else(|| panic!("no redirect: {} {}", r.status, r.text()));
    let u = url::Url::parse(loc).unwrap();
    (
        format!("{}://{}{}", u.scheme(), u.host_str().unwrap(), u.path()),
        u.query_pairs().into_owned().collect(),
    )
}

/// Run consent and return the authorization code.
async fn approve(
    app: &mut TestApp,
    client_id: &str,
    redirect: &str,
    scope: Option<&str>,
) -> String {
    let q = authorize_query(client_id, redirect, scope, "st-1");
    let page = app.get(&format!("/oauth/authorize?{q}")).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    let html = page.text();
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_string();
    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(q.as_bytes())
        .into_owned()
        .collect();
    pairs.push(("csrf".into(), csrf));
    pairs.push(("decision".into(), "approve".into()));
    let refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let r = post_form(app, "/oauth/authorize", &refs, &[]).await;
    let (_, params) = location_params(&r);
    assert_eq!(params["state"], "st-1");
    assert_eq!(params["iss"], "http://localhost:8080");
    params["code"].clone()
}

async fn exchange(app: &mut TestApp, client_id: &str, redirect: &str, code: &str) -> Reply {
    post_form(
        app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect),
            ("code_verifier", VERIFIER),
            ("client_id", client_id),
        ],
        &[],
    )
    .await
}

async fn refresh(app: &mut TestApp, client_id: &str, token: &str) -> Reply {
    post_form(
        app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", token),
            ("client_id", client_id),
        ],
        &[],
    )
    .await
}

// ---- discovery -------------------------------------------------------------------------------

#[tokio::test]
async fn discovery_documents() {
    let mut app = app().await;
    for p in [
        "/.well-known/oauth-protected-resource",
        "/.well-known/oauth-protected-resource/mcp",
    ] {
        let j = app.get(p).await.json();
        assert_eq!(
            j,
            json!({
                "resource": "http://localhost:8080/mcp",
                "authorization_servers": ["http://localhost:8080"],
                "scopes_supported": ["calendar:read", "calendar:write"],
                "bearer_methods_supported": ["header"]
            }),
            "{p}"
        );
    }
    let j = app
        .get("/.well-known/oauth-authorization-server")
        .await
        .json();
    assert_eq!(j["issuer"], "http://localhost:8080");
    assert_eq!(
        j["authorization_endpoint"],
        "http://localhost:8080/oauth/authorize"
    );
    assert_eq!(j["token_endpoint"], "http://localhost:8080/oauth/token");
    assert_eq!(
        j["registration_endpoint"],
        "http://localhost:8080/oauth/register"
    );
    assert_eq!(
        j["revocation_endpoint"],
        "http://localhost:8080/oauth/revoke"
    );
    assert_eq!(j["response_types_supported"], json!(["code"]));
    assert_eq!(
        j["grant_types_supported"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(j["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(
        j["token_endpoint_auth_methods_supported"],
        json!(["none", "client_secret_post", "client_secret_basic"])
    );
    assert_eq!(
        j["scopes_supported"],
        json!(["calendar:read", "calendar:write", "offline_access"])
    );
    assert_eq!(j["client_id_metadata_document_supported"], true);
}

// ---- registration ----------------------------------------------------------------------------

#[tokio::test]
async fn dynamic_client_registration() {
    let mut app = app().await;
    let r = register(
        &mut app,
        json!({"client_name": "Claude", "redirect_uris": [CB]}),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let j = r.json();
    assert!(j["client_id"].as_str().unwrap().starts_with("fy_client_"));
    assert_eq!(j["token_endpoint_auth_method"], "none");
    assert!(j.get("client_secret").is_none());
    assert_eq!(j["redirect_uris"], json!([CB]));

    let r = register(&mut app, json!({"client_name": "Grok", "redirect_uris": ["http://127.0.0.1:8000/cb"], "token_endpoint_auth_method": "client_secret_basic"})).await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert!(
        r.json()["client_secret"]
            .as_str()
            .unwrap()
            .starts_with("fy_secret_")
    );

    for (body, code) in [
        (json!({"client_name": "x"}), "invalid_redirect_uri"),
        (json!({"redirect_uris": []}), "invalid_redirect_uri"),
        (
            json!({"redirect_uris": ["http://evil.example/cb"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": ["javascript:alert(1)"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": [CB], "token_endpoint_auth_method": "private_key_jwt"}),
            "invalid_client_metadata",
        ),
    ] {
        let r = register(&mut app, body.clone()).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(r.json()["error"], code, "{body}");
    }
    let r = app
        .send(
            Request::post("/oauth/register")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn registration_is_rate_limited_and_capped() {
    let mut a = app().await;
    let mut statuses = vec![];
    for _ in 0..11 {
        statuses.push(
            register(&mut a, json!({"redirect_uris": [CB]}))
                .await
                .status,
        );
    }
    assert!(statuses[..10].iter().all(|s| *s == StatusCode::CREATED));
    assert_eq!(
        statuses[10],
        StatusCode::TOO_MANY_REQUESTS,
        "10 per hour per IP"
    );

    // 100 clients maximum: fill the table directly, then try to register one more.
    let mut b = app().await;
    for i in 0..100 {
        sqlx::query("INSERT INTO oauth_clients (client_id, kind, client_name, redirect_uris, created_at) VALUES (?, 'dcr', 'c', '[]', ?)")
            .bind(format!("c{i}"))
            .bind(annee_server::storage::now_ts())
            .execute(&b.state.db.pool)
            .await
            .unwrap();
    }
    let r = register(&mut b, json!({"redirect_uris": [CB]})).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.text());
}

// ---- CIMD ------------------------------------------------------------------------------------

#[tokio::test]
async fn client_id_metadata_documents() {
    let docs = MockServer::start().await;
    let url = format!("{}/client.json", docs.uri());
    Mock::given(method("GET")).and(path("/client.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "client_id": url, "client_name": "Agent CIMD", "redirect_uris": [CB], "token_endpoint_auth_method": "none"
        })))
        .expect(1) // fetched once, then cached for 24 h
        .mount(&docs).await;
    let mut app = app().await;
    app.login().await;
    let page = app
        .get(&format!(
            "/oauth/authorize?{}",
            authorize_query(&url, CB, None, "s")
        ))
        .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    assert!(page.text().contains("Agent CIMD"));
    // Second request is served from the cache.
    assert_eq!(
        app.get(&format!(
            "/oauth/authorize?{}",
            authorize_query(&url, CB, None, "s")
        ))
        .await
        .status,
        StatusCode::OK
    );

    // The full flow works with a CIMD client_id.
    let code = approve(&mut app, &url, CB, None).await;
    let t = exchange(&mut app, &url, CB, &code).await;
    assert_eq!(t.status, StatusCode::OK, "{}", t.text());
    let claims = t.json()["access_token"].as_str().unwrap().to_string();
    let payload: Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(claims.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(payload["client_id"], url.as_str());
    docs.verify().await;
}

#[tokio::test]
async fn invalid_client_metadata_documents_are_refused() {
    let docs = MockServer::start().await;
    let mismatch = format!("{}/mismatch.json", docs.uri());
    Mock::given(method("GET")).and(path("/mismatch.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"client_id": "https://other.example/c.json", "client_name": "x", "redirect_uris": [CB]})))
        .mount(&docs).await;
    let bad_uri = format!("{}/baduri.json", docs.uri());
    Mock::given(method("GET"))
        .and(path("/baduri.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"client_id": bad_uri, "redirect_uris": ["http://evil.example/cb"]}),
        ))
        .mount(&docs)
        .await;
    let big = format!("{}/big.json", docs.uri());
    Mock::given(method("GET"))
        .and(path("/big.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "{{\"client_id\":\"{big}\",\"pad\":\"{}\"}}",
            "x".repeat(70_000)
        )))
        .mount(&docs)
        .await;
    let mut app = app().await;
    app.login().await;
    for id in [
        mismatch,
        bad_uri,
        big,
        "https://127.0.0.1/c.json".to_string(),
        "https://10.0.0.5/c.json".to_string(),
    ] {
        let r = app
            .get(&format!(
                "/oauth/authorize?{}",
                authorize_query(&id, CB, None, "s")
            ))
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{id}: {}", r.text());
        assert!(r.header("location").is_none());
    }
}

// ---- authorization endpoint ------------------------------------------------------------------

#[tokio::test]
async fn authorize_requires_login_and_returns_afterwards() {
    let mut app = app().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let q = authorize_query(&cid, CB, None, "s1");
    let r = app.get(&format!("/oauth/authorize?{q}")).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let loc = r.header("location").unwrap();
    assert!(
        loc.starts_with("/connexion?next=%2Foauth%2Fauthorize%3F"),
        "{loc}"
    );
    // After signing in with that `next`, the consent page shows.
    app.login().await;
    assert_eq!(
        app.get(&format!("/oauth/authorize?{q}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn consent_page_texts() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude <b>", CB).await;
    let html = app
        .get(&format!(
            "/oauth/authorize?{}",
            authorize_query(&cid, CB, None, "s")
        ))
        .await
        .text();
    assert!(
        html.contains("<strong>Claude &lt;b&gt;</strong> demande l’accès à l’agenda familial."),
        "{html}"
    );
    assert!(html.contains("Voir les événements"));
    assert!(html.contains("Ajouter, modifier et supprimer des événements"));
    assert!(html.contains("Vous serez redirigé vers <strong>claude.ai</strong>"));
    assert!(html.contains("Autoriser") && html.contains("Refuser"));
    assert!(!html.contains("adresse locale"));
    let html = app
        .get(&format!(
            "/oauth/authorize?{}",
            authorize_query(&cid, CB, Some("calendar:read"), "s")
        ))
        .await
        .text();
    assert!(
        !html.contains("Ajouter, modifier"),
        "read-only request lists only viewing"
    );
    // Loopback clients get a warning.
    let lb = public_client(&mut app, "CLI", "http://localhost:3000/cb").await;
    let html = app
        .get(&format!(
            "/oauth/authorize?{}",
            authorize_query(&lb, "http://localhost:3000/cb", None, "s")
        ))
        .await
        .text();
    assert!(html.contains("Cette application tourne sur un ordinateur (adresse locale). Ne continuez que si vous venez de la lancer."));
}

#[tokio::test]
async fn redirect_uri_rules() {
    let mut app = app().await;
    app.login().await;
    let lh = public_client(&mut app, "A", "http://localhost:3000/cb").await;
    let ip = public_client(&mut app, "B", "http://127.0.0.1:3000/cb").await;
    let fixed = public_client(&mut app, "C", CB).await;
    let ok = |c: &str, r: &str| {
        let q = authorize_query(c, r, None, "s");
        format!("/oauth/authorize?{q}")
    };
    // Loopback: any port, for localhost and 127.0.0.1.
    assert_eq!(
        app.get(&ok(&lh, "http://localhost:54321/cb")).await.status,
        StatusCode::OK
    );
    assert_eq!(
        app.get(&ok(&ip, "http://127.0.0.1:1/cb")).await.status,
        StatusCode::OK
    );
    // But not across hosts or paths, and exact everywhere else.
    assert_eq!(
        app.get(&ok(&lh, "http://127.0.0.1:3000/cb")).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.get(&ok(&lh, "http://localhost:3000/other"))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.get(&ok(&fixed, "https://claude.ai/api/mcp/auth_callback2"))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.get(&ok(&fixed, "https://evil.example/cb")).await.status,
        StatusCode::BAD_REQUEST
    );
    // Unknown client.
    assert_eq!(
        app.get(&ok("nope", CB)).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn protocol_errors_redirect_with_state_and_issuer() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let ch = pkce_challenge(VERIFIER);
    let base = |extra: &[(&str, &str)]| {
        let mut p = vec![
            ("response_type", "code"),
            ("client_id", &cid[..]),
            ("redirect_uri", CB),
            ("state", "xyz"),
        ];
        p.extend_from_slice(extra);
        format!("/oauth/authorize?{}", form(&p))
    };
    for (uri, err) in [
        (base(&[]), "invalid_request"), // no PKCE
        (
            base(&[("code_challenge", &ch), ("code_challenge_method", "plain")]),
            "invalid_request",
        ),
        (
            base(&[
                ("code_challenge", &ch),
                ("code_challenge_method", "S256"),
                ("scope", "admin"),
            ]),
            "invalid_scope",
        ),
        (
            base(&[
                ("code_challenge", &ch),
                ("code_challenge_method", "S256"),
                ("resource", "https://other.example/mcp"),
            ]),
            "invalid_target",
        ),
    ] {
        let r = app.get(&uri).await;
        let (_, p) = location_params(&r);
        assert_eq!(p["error"], err, "{uri}");
        assert_eq!(p["state"], "xyz");
        assert_eq!(p["iss"], "http://localhost:8080");
    }
    let r = app
        .get(&format!(
            "/oauth/authorize?{}",
            form(&[
                ("response_type", "token"),
                ("client_id", &cid),
                ("redirect_uri", CB),
                ("code_challenge", &ch),
                ("code_challenge_method", "S256")
            ])
        ))
        .await;
    assert_eq!(location_params(&r).1["error"], "unsupported_response_type");
    // A matching `resource` is accepted.
    let ok = base(&[
        ("code_challenge", &ch),
        ("code_challenge_method", "S256"),
        ("resource", "http://localhost:8080/mcp"),
    ]);
    assert_eq!(app.get(&ok).await.status, StatusCode::OK);
}

#[tokio::test]
async fn denying_returns_access_denied() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let q = authorize_query(&cid, CB, None, "keep-me");
    let html = app.get(&format!("/oauth/authorize?{q}")).await.text();
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_string();
    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(q.as_bytes())
        .into_owned()
        .collect();
    pairs.push(("csrf".into(), csrf));
    pairs.push(("decision".into(), "deny".into()));
    let refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let r = post_form(&mut app, "/oauth/authorize", &refs, &[]).await;
    let (base, p) = location_params(&r);
    assert_eq!(base, "https://claude.ai/api/mcp/auth_callback");
    assert_eq!(
        (p["error"].as_str(), p["state"].as_str()),
        ("access_denied", "keep-me")
    );
    assert!(!p.contains_key("code"));
}

#[tokio::test]
async fn consent_post_needs_the_csrf_token_and_a_session() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let q = authorize_query(&cid, CB, None, "s");
    app.get(&format!("/oauth/authorize?{q}")).await;
    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(q.as_bytes())
        .into_owned()
        .collect();
    pairs.push(("csrf".into(), "forged".into()));
    pairs.push(("decision".into(), "approve".into()));
    let refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        post_form(&mut app, "/oauth/authorize", &refs, &[])
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    // Cross-site form post.
    assert_eq!(
        post_form(
            &mut app,
            "/oauth/authorize",
            &refs,
            &[
                ("origin", "https://evil.example"),
                ("host", "localhost:8080")
            ]
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    app.clear_cookies();
    assert_eq!(
        post_form(&mut app, "/oauth/authorize", &refs, &[])
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

// ---- token endpoint --------------------------------------------------------------------------

#[tokio::test]
async fn code_exchange_issues_a_signed_access_token_and_a_refresh_token() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let code = approve(&mut app, &cid, CB, None).await;
    let r = exchange(&mut app, &cid, CB, &code).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.header("cache-control"), Some("no-store"));
    let j = r.json();
    assert_eq!(j["token_type"], "Bearer");
    assert_eq!(j["expires_in"], 3600);
    assert_eq!(j["scope"], "calendar:read calendar:write");
    assert!(j["refresh_token"].as_str().unwrap().len() >= 43);

    let tok = j["access_token"].as_str().unwrap();
    assert_eq!(
        jsonwebtoken::decode_header(tok).unwrap().alg,
        jsonwebtoken::Algorithm::EdDSA
    );
    let claims: AccessClaims = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(tok.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims.iss, "http://localhost:8080");
    assert_eq!(claims.aud, "http://localhost:8080/mcp");
    assert_eq!(claims.sub, "alex@demo.local");
    assert_eq!(claims.client_id, cid);
    assert_eq!(claims.scope, "calendar:read calendar:write");
    assert!(!claims.grant_id.is_empty() && !claims.jti.is_empty());
    assert_eq!(claims.exp - claims.iat, 3600);
    // The refresh token is stored hashed.
    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM oauth_refresh_tokens")
        .fetch_one(&app.state.db.pool)
        .await
        .unwrap();
    assert_eq!(
        stored,
        annee_server::crypto::sha256(j["refresh_token"].as_str().unwrap().as_bytes())
    );
}

#[tokio::test]
async fn token_endpoint_errors() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    // JSON bodies are refused: form-urlencoded only.
    let r = app
        .send(
            Request::post("/oauth/token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"grant_type": "refresh_token"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json()["error"], "invalid_request");
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[("grant_type", "password"), ("client_id", &cid)],
        &[],
    )
    .await;
    assert_eq!(r.json()["error"], "unsupported_grant_type");
    let r = post_form(&mut app, "/oauth/token", &[("client_id", &cid)], &[]).await;
    assert_eq!(r.json()["error"], "invalid_request");
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", "ghost"),
            ("code", "x"),
            ("redirect_uri", CB),
            ("code_verifier", VERIFIER),
        ],
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.json()["error"], "invalid_client");
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &cid),
            ("code", "unknown"),
            ("redirect_uri", CB),
            ("code_verifier", VERIFIER),
        ],
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json()["error"], "invalid_grant");

    // Wrong verifier, wrong redirect_uri, reuse, other client.
    let code = approve(&mut app, &cid, CB, None).await;
    let bad_verifier = "x".repeat(43);
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &cid),
            ("code", &code),
            ("redirect_uri", CB),
            ("code_verifier", &bad_verifier),
        ],
        &[],
    )
    .await;
    assert_eq!(r.json()["error"], "invalid_grant");
    let r = exchange(&mut app, &cid, "https://claude.ai/other", &code).await;
    assert_eq!(r.json()["error"], "invalid_grant");
    let other = public_client(&mut app, "Other", "https://other.example/cb").await;
    assert_eq!(
        exchange(&mut app, &other, CB, &code).await.json()["error"],
        "invalid_grant"
    );
    assert_eq!(
        exchange(&mut app, &cid, CB, &code).await.status,
        StatusCode::OK,
        "failed attempts do not burn the code"
    );
    let r = exchange(&mut app, &cid, CB, &code).await;
    assert_eq!(r.json()["error"], "invalid_grant", "single use");

    // Wrong resource.
    let code = approve(&mut app, &cid, CB, None).await;
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &cid),
            ("code", &code),
            ("redirect_uri", CB),
            ("code_verifier", VERIFIER),
            ("resource", "https://x.example/mcp"),
        ],
        &[],
    )
    .await;
    assert_eq!(r.json()["error"], "invalid_target");
}

#[tokio::test]
async fn expired_codes_are_refused() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let code = approve(&mut app, &cid, CB, None).await;
    sqlx::query("UPDATE oauth_codes SET expires_at = '2020-01-01T00:00:00Z'")
        .execute(&app.state.db.pool)
        .await
        .unwrap();
    assert_eq!(
        exchange(&mut app, &cid, CB, &code).await.json()["error"],
        "invalid_grant"
    );
}

#[tokio::test]
async fn confidential_clients_authenticate_with_post_or_basic() {
    let mut app = app().await;
    app.login().await;
    let r = register(&mut app, json!({"client_name": "Grok", "redirect_uris": [CB], "token_endpoint_auth_method": "client_secret_post"})).await.json();
    let (cid, secret) = (
        r["client_id"].as_str().unwrap().to_string(),
        r["client_secret"].as_str().unwrap().to_string(),
    );
    let code = approve(&mut app, &cid, CB, None).await;
    // Missing/wrong secret.
    let r = exchange(&mut app, &cid, CB, &code).await;
    assert_eq!(
        (r.status, r.json()["error"].as_str().map(str::to_string)),
        (StatusCode::UNAUTHORIZED, Some("invalid_client".into()))
    );
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &cid),
            ("client_secret", "nope"),
            ("code", &code),
            ("redirect_uri", CB),
            ("code_verifier", VERIFIER),
        ],
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    // client_secret_post.
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &cid),
            ("client_secret", &secret),
            ("code", &code),
            ("redirect_uri", CB),
            ("code_verifier", VERIFIER),
        ],
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    // client_secret_basic on refresh.
    let refresh_token = r.json()["refresh_token"].as_str().unwrap().to_string();
    let basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{cid}:{secret}"))
    );
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh_token),
        ],
        &[("authorization", &basic)],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
}

#[tokio::test]
async fn refresh_tokens_rotate_and_reuse_revokes_the_grant() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let code = approve(&mut app, &cid, CB, None).await;
    let first = exchange(&mut app, &cid, CB, &code).await.json();
    let rt1 = first["refresh_token"].as_str().unwrap().to_string();

    let started = std::time::Instant::now();
    let second = refresh(&mut app, &cid, &rt1).await;
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "refresh answers quickly"
    );
    assert_eq!(second.status, StatusCode::OK, "{}", second.text());
    let j2 = second.json();
    let rt2 = j2["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(rt1, rt2, "rotated");
    assert_eq!(j2["scope"], "calendar:read calendar:write");

    let rt3 = refresh(&mut app, &cid, &rt2).await.json()["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Presenting an already-rotated token revokes the whole grant, including the latest token.
    let reuse = refresh(&mut app, &cid, &rt1).await;
    assert_eq!(reuse.status, StatusCode::BAD_REQUEST);
    assert_eq!(reuse.json()["error"], "invalid_grant");
    assert_eq!(
        refresh(&mut app, &cid, &rt3).await.json()["error"],
        "invalid_grant"
    );
    assert_eq!(app.get("/api/admin/oauth-grants").await.json(), json!([]));
    // And the access tokens of that grant stop working on /mcp.
    let base = spawn_server(app.state.clone()).await;
    let r = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .bearer_auth(j2["access_token"].as_str().unwrap())
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn refresh_scope_can_only_narrow() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let code = approve(&mut app, &cid, CB, Some("calendar:read")).await;
    let rt = exchange(&mut app, &cid, CB, &code).await.json()["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();
    let r = post_form(
        &mut app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &rt),
            ("client_id", &cid),
            ("scope", "calendar:read calendar:write"),
        ],
        &[],
    )
    .await;
    assert_eq!(r.json()["error"], "invalid_scope");
}

#[tokio::test]
async fn expired_refresh_tokens_and_other_clients_are_refused() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let other = public_client(&mut app, "Other", "https://other.example/cb").await;
    let code = approve(&mut app, &cid, CB, None).await;
    let rt = exchange(&mut app, &cid, CB, &code).await.json()["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        refresh(&mut app, &other, &rt).await.json()["error"],
        "invalid_grant"
    );
    sqlx::query("UPDATE oauth_refresh_tokens SET expires_at = '2020-01-01T00:00:00Z'")
        .execute(&app.state.db.pool)
        .await
        .unwrap();
    assert_eq!(
        refresh(&mut app, &cid, &rt).await.json()["error"],
        "invalid_grant"
    );
    assert_eq!(
        refresh(&mut app, &cid, "garbage").await.json()["error"],
        "invalid_grant"
    );
    app.state.oauth.purge_expired().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_refresh_tokens")
        .fetch_one(&app.state.db.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "expired rows are purged");
}

// ---- revocation, grants, MCP access ----------------------------------------------------------

#[tokio::test]
async fn revoke_endpoint_revokes_the_grant() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let code = approve(&mut app, &cid, CB, None).await;
    let j = exchange(&mut app, &cid, CB, &code).await.json();
    let rt = j["refresh_token"].as_str().unwrap();
    // Unknown tokens still answer 200 (RFC 7009).
    assert_eq!(
        post_form(
            &mut app,
            "/oauth/revoke",
            &[("token", "whatever"), ("client_id", &cid)],
            &[]
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        post_form(
            &mut app,
            "/oauth/revoke",
            &[
                ("token", rt),
                ("client_id", &cid),
                ("token_type_hint", "refresh_token")
            ],
            &[]
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        refresh(&mut app, &cid, rt).await.json()["error"],
        "invalid_grant"
    );
    assert_eq!(app.get("/api/admin/oauth-grants").await.json(), json!([]));
}

#[tokio::test]
async fn mcp_works_with_oauth_tokens_and_scopes_are_enforced() {
    let mut app = app().await;
    app.login().await;
    let base = spawn_server(app.state.clone()).await;

    // Read + write.
    let cid = public_client(&mut app, "Claude Desktop", CB).await;
    let code = approve(&mut app, &cid, CB, None).await;
    let access = exchange(&mut app, &cid, CB, &code).await.json()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let client = mcp(&base, &access).await;
    let ctx = call(&client, "get_context", json!({})).await;
    assert_eq!(ctx.structured_content.unwrap()["today"], "2026-10-16");
    let created = call(
        &client,
        "create_event",
        json!({"type": "celebration", "title": "Fête OAuth", "start": "2026-11-14"}),
    )
    .await;
    assert_eq!(
        created.structured_content.unwrap()["created_by"],
        "Claude Desktop",
        "the OAuth client name is the writer"
    );
    client.cancel().await.ok();

    // Read only.
    let code = approve(&mut app, &cid, CB, Some("calendar:read")).await;
    let access = exchange(&mut app, &cid, CB, &code).await.json()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let client = mcp(&base, &access).await;
    assert!(
        call(
            &client,
            "list_events",
            json!({"from": "2026-11-01", "to": "2026-11-30"})
        )
        .await
        .is_error
            != Some(true)
    );
    let denied = call(
        &client,
        "create_event",
        json!({"type": "celebration", "title": "Non", "start": "2026-11-15"}),
    )
    .await;
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(
        denied.structured_content.unwrap()["code"],
        "insufficient_scope"
    );
    client.cancel().await.ok();
}

#[tokio::test]
async fn grants_can_be_listed_and_revoked_in_settings() {
    let mut app = app().await;
    app.login().await;
    let base = spawn_server(app.state.clone()).await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let code = approve(&mut app, &cid, CB, None).await;
    let access = exchange(&mut app, &cid, CB, &code).await.json()["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    let list = app.get("/api/admin/oauth-grants").await.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    let g = &list[0];
    assert_eq!(g["client_name"], "Claude");
    assert_eq!(g["redirect_host"], "claude.ai");
    assert_eq!(g["scope"], "calendar:read calendar:write");
    assert!(g["created_at"].is_string());
    assert_eq!(g["last_used_at"], Value::Null);

    let client = mcp(&base, &access).await;
    call(&client, "get_context", json!({})).await;
    client.cancel().await.ok();
    assert!(app.get("/api/admin/oauth-grants").await.json()[0]["last_used_at"].is_string());

    let id = g["id"].as_str().unwrap();
    assert_eq!(
        app.delete(&format!("/api/admin/oauth-grants/{id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.delete(&format!("/api/admin/oauth-grants/{id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let r = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .bearer_auth(&access)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        401,
        "access tokens of a revoked grant are refused"
    );
    // …and so is its refresh token.
    assert_eq!(app.get("/api/admin/oauth-grants").await.json(), json!([]));
}

#[tokio::test]
async fn forged_or_foreign_access_tokens_are_refused_on_mcp() {
    let mut app = app().await;
    app.login().await;
    let base = spawn_server(app.state.clone()).await;
    // Signed with another key.
    let other = annee_server::oauth::jwt::TokenKeys::random();
    let now = chrono::Utc::now().timestamp();
    let claims = AccessClaims {
        iss: "http://localhost:8080".into(),
        aud: "http://localhost:8080/mcp".into(),
        sub: "x".into(),
        client_id: "c".into(),
        client_name: "Evil".into(),
        scope: "calendar:write".into(),
        grant_id: "g".into(),
        jti: "j".into(),
        iat: now,
        exp: now + 3600,
    };
    let forged = other.sign(&claims).unwrap();
    // Right key, wrong audience.
    let wrong_aud = app
        .state
        .oauth
        .keys
        .sign(&AccessClaims {
            aud: "http://localhost:8080/other".into(),
            ..claims.clone()
        })
        .unwrap();
    // Right key, expired.
    let expired = app
        .state
        .oauth
        .keys
        .sign(&AccessClaims {
            exp: now - 100,
            iat: now - 4000,
            ..claims.clone()
        })
        .unwrap();
    for t in [forged, wrong_aud, expired] {
        let r = reqwest::Client::new()
            .post(format!("{base}/mcp"))
            .bearer_auth(&t)
            .header("content-type", "application/json")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
        assert!(
            r.headers()
                .get("www-authenticate")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("oauth-protected-resource")
        );
    }
}

async fn mcp(base: &str, token: &str) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let t = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("{base}/mcp"))
            .auth_header(token.to_string()),
    );
    ().serve(t).await.expect("connects")
}

async fn call(
    c: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tool: &str,
    args: Value,
) -> rmcp::model::CallToolResult {
    c.call_tool(
        CallToolRequestParams::new(tool.to_string())
            .with_arguments(args.as_object().cloned().unwrap_or_default()),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn consent_decision_csp_allows_the_cross_origin_redirect() {
    let mut app = app().await;
    app.login().await;
    let cid = public_client(&mut app, "Claude", CB).await;
    let q = authorize_query(&cid, CB, None, "s");
    let page = app.get(&format!("/oauth/authorize?{q}")).await;
    assert!(
        !page
            .header("content-security-policy")
            .unwrap()
            .contains("form-action")
    );
    let csrf = page
        .text()
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_string();
    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(q.as_bytes())
        .into_owned()
        .collect();
    pairs.push(("csrf".into(), csrf));
    pairs.push(("decision".into(), "approve".into()));
    let refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let r = post_form(&mut app, "/oauth/authorize", &refs, &[]).await;
    let csp = r.header("content-security-policy").unwrap();
    assert!(!csp.contains("form-action") && csp.contains("default-src 'self'"));
}
