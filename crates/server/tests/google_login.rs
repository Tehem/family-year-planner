//! Google Sign-In (§11.1) against a mocked Google.

mod common;

use annee_server::crypto::pkce_challenge;
use annee_server::google_login::Endpoints;
use axum::http::StatusCode;
use common::{Reply, TestApp, prod_like_state, prod_like_state_with};
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static PRIVATE_PEM: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| generate_pem().0);

struct Google {
    server: MockServer,
}

impl Google {
    async fn start() -> Google {
        let server = MockServer::start().await;
        let key = EncodingKey::from_rsa_pem(PRIVATE_PEM.as_bytes()).unwrap();
        let mut jwk = Jwk::from_encoding_key(&key, Algorithm::RS256).unwrap();
        jwk.common.key_id = Some("test-key-1".into());
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(JwkSet { keys: vec![jwk] }))
            .mount(&server)
            .await;
        Google { server }
    }

    fn endpoints(&self) -> Endpoints {
        Endpoints {
            auth_url: format!("{}/auth", self.server.uri()),
            token_url: format!("{}/token", self.server.uri()),
            jwks_url: format!("{}/certs", self.server.uri()),
        }
    }

    fn id_token(&self, claims: Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key-1".into());
        jsonwebtoken::encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(PRIVATE_PEM.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    fn claims(&self, nonce: &str, email: &str) -> Value {
        let now = chrono::Utc::now().timestamp();
        json!({
            "iss": "https://accounts.google.com", "aud": "client-id", "sub": "123",
            "email": email, "email_verified": true, "nonce": nonce, "iat": now, "exp": now + 3600
        })
    }

    async fn serve_token(&self, id_token: String) {
        self.server.reset().await;
        // `reset` removes the JWKS mock too: restore it.
        let key = EncodingKey::from_rsa_pem(PRIVATE_PEM.as_bytes()).unwrap();
        let mut jwk = Jwk::from_encoding_key(&key, Algorithm::RS256).unwrap();
        jwk.common.key_id = Some("test-key-1".into());
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(JwkSet { keys: vec![jwk] }))
            .mount(&self.server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id_token": id_token, "access_token": "x"})),
            )
            .mount(&self.server)
            .await;
    }
}

struct Start {
    state: String,
    nonce: String,
    challenge: String,
    location: String,
}

async fn begin(app: &mut TestApp, next: Option<&str>) -> Start {
    let uri = match next {
        Some(n) => format!("/auth/google/start?next={n}"),
        None => "/auth/google/start".to_string(),
    };
    let r = app.get(&uri).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.text());
    let location = r.header("location").unwrap().to_string();
    let url = url::Url::parse(&location).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    Start {
        state: q["state"].clone(),
        nonce: q["nonce"].clone(),
        challenge: q["code_challenge"].clone(),
        location,
    }
}

async fn callback(app: &mut TestApp, state: &str) -> Reply {
    app.get(&format!("/auth/google/callback?code=abc&state={state}"))
        .await
}

async fn setup() -> (TestApp, Google) {
    let google = Google::start().await;
    let app = TestApp::with_state(prod_like_state(google.endpoints()).await);
    (app, google)
}

#[tokio::test]
async fn start_redirects_to_google_with_pkce_and_scopes() {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, Some("/reglages")).await;
    assert!(
        s.location
            .starts_with(&format!("{}/auth?", google.server.uri()))
    );
    let url = url::Url::parse(&s.location).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q["client_id"], "client-id");
    assert_eq!(
        q["redirect_uri"],
        "https://famille.example.org/auth/google/callback"
    );
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["scope"], "openid email");
    assert_eq!(q["code_challenge_method"], "S256");
    assert!(!s.state.is_empty() && !s.nonce.is_empty() && !s.challenge.is_empty());
}

#[tokio::test]
async fn happy_path_creates_a_session_and_returns_to_next() {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, Some("/reglages")).await;
    google
        .serve_token(google.id_token(google.claims(&s.nonce, "Alex@example.com")))
        .await;
    let r = callback(&mut app, &s.state).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.header("location"), Some("/reglages"));

    // The code was exchanged with the PKCE verifier matching the challenge, plus the secret.
    let reqs = google.server.received_requests().await.unwrap();
    let token_req = reqs.iter().find(|r| r.url.path() == "/token").unwrap();
    let body = String::from_utf8_lossy(&token_req.body).to_string();
    let form: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["code"], "abc");
    assert_eq!(form["client_secret"], "client-secret");
    assert_eq!(
        form["redirect_uri"],
        "https://famille.example.org/auth/google/callback"
    );
    assert_eq!(pkce_challenge(&form["code_verifier"]), s.challenge);

    let me = app.get("/api/me").await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["name"], "Alex");
    assert_eq!(me.json()["email"], "alex@example.com");
    assert_eq!(me.json()["demo"], false);
    // The sign-in works for the API and the pages.
    assert_eq!(app.get("/reglages").await.status, StatusCode::OK);
}

#[tokio::test]
async fn allowlist_is_case_insensitive_and_names_default_to_the_local_part() {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, None).await;
    google
        .serve_token(google.id_token(google.claims(&s.nonce, "camille@example.com")))
        .await;
    let r = callback(&mut app, &s.state).await;
    assert_eq!(r.header("location"), Some("/"));
    assert_eq!(app.get("/api/me").await.json()["name"], "camille");
}

#[tokio::test]
async fn emails_outside_the_allowlist_are_refused_with_their_address() {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, None).await;
    google
        .serve_token(google.id_token(google.claims(&s.nonce, "intrus@example.org")))
        .await;
    let r = callback(&mut app, &s.state).await;
    assert_eq!(
        r.header("location"),
        Some("/connexion?error=not_allowed&email=intrus%40example.org")
    );
    assert_eq!(app.get("/api/me").await.status, StatusCode::UNAUTHORIZED);
}

async fn expect_failure(claims_edit: impl FnOnce(&mut Value), expected: &str) {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, None).await;
    let mut claims = google.claims(&s.nonce, "alex@example.com");
    claims_edit(&mut claims);
    google.serve_token(google.id_token(claims)).await;
    let r = callback(&mut app, &s.state).await;
    assert_eq!(r.header("location"), Some(expected));
    assert_eq!(app.get("/api/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_id_tokens_are_rejected() {
    expect_failure(
        |c| c["aud"] = json!("someone-else"),
        "/connexion?error=failed",
    )
    .await;
    expect_failure(
        |c| c["iss"] = json!("https://evil.example"),
        "/connexion?error=failed",
    )
    .await;
    expect_failure(
        |c| c["exp"] = json!(chrono::Utc::now().timestamp() - 3600),
        "/connexion?error=failed",
    )
    .await;
    expect_failure(|c| c["nonce"] = json!("other"), "/connexion?error=failed").await;
    expect_failure(
        |c| c["email_verified"] = json!(false),
        "/connexion?error=failed",
    )
    .await;
    expect_failure(
        |c| {
            c.as_object_mut().unwrap().remove("email");
        },
        "/connexion?error=failed",
    )
    .await;
    // Google sometimes sends booleans as strings.
    let (mut app, google) = setup().await;
    let s = begin(&mut app, None).await;
    let mut claims = google.claims(&s.nonce, "alex@example.com");
    claims["email_verified"] = json!("true");
    google.serve_token(google.id_token(claims)).await;
    assert_eq!(
        callback(&mut app, &s.state).await.header("location"),
        Some("/")
    );
}

#[tokio::test]
async fn tokens_signed_with_another_key_are_rejected() {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, None).await;
    // Same kid, different key: signature must fail.
    let other = generate_pem().0;
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key-1".into());
    let token = jsonwebtoken::encode(
        &header,
        &google.claims(&s.nonce, "alex@example.com"),
        &EncodingKey::from_rsa_pem(other.as_bytes()).unwrap(),
    )
    .unwrap();
    google.serve_token(token).await;
    assert_eq!(
        callback(&mut app, &s.state).await.header("location"),
        Some("/connexion?error=failed")
    );
}

#[tokio::test]
async fn state_is_checked_and_the_flow_is_single_use() {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, None).await;
    google
        .serve_token(google.id_token(google.claims(&s.nonce, "alex@example.com")))
        .await;
    let r = callback(&mut app, "wrong-state").await;
    assert_eq!(r.header("location"), Some("/connexion?error=invalid"));
    // The flow cookie was consumed by the failed attempt.
    let r = callback(&mut app, &s.state).await;
    assert_eq!(r.header("location"), Some("/connexion?error=invalid"));
    // No cookie at all.
    let mut fresh = TestApp::with_state(prod_like_state(google.endpoints()).await);
    assert_eq!(
        callback(&mut fresh, "x").await.header("location"),
        Some("/connexion?error=invalid")
    );
}

#[tokio::test]
async fn user_cancelling_at_google_is_reported() {
    let (mut app, _google) = setup().await;
    let s = begin(&mut app, None).await;
    let r = app
        .get(&format!(
            "/auth/google/callback?error=access_denied&state={}",
            s.state
        ))
        .await;
    assert_eq!(r.header("location"), Some("/connexion?error=denied"));
}

#[tokio::test]
async fn next_must_be_relative() {
    let (mut app, google) = setup().await;
    let s = begin(&mut app, Some("//evil.example")).await;
    google
        .serve_token(google.id_token(google.claims(&s.nonce, "alex@example.com")))
        .await;
    assert_eq!(
        callback(&mut app, &s.state).await.header("location"),
        Some("/")
    );
}

#[tokio::test]
async fn demo_login_does_not_exist_in_production_and_google_login_not_in_demo() {
    let (mut app, _google) = setup().await;
    let r = app
        .send(
            axum::http::Request::post("/auth/demo")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let mut demo = TestApp::demo().await;
    assert_eq!(
        demo.get("/auth/google/start").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn revoking_an_email_from_the_allowlist_ends_its_sessions() {
    let google = Google::start().await;
    let key = axum_extra::extract::cookie::Key::generate();
    let state = prod_like_state_with(
        google.endpoints(),
        key.clone(),
        vec!["alex@example.com".into()],
    )
    .await;
    let mut app = TestApp::with_state(state);
    let s = begin(&mut app, None).await;
    google
        .serve_token(google.id_token(google.claims(&s.nonce, "alex@example.com")))
        .await;
    callback(&mut app, &s.state).await;
    assert_eq!(app.get("/api/me").await.status, StatusCode::OK);

    // Same cookie, same key, but Alex has since been removed from the allowlist.
    let strict =
        prod_like_state_with(google.endpoints(), key, vec!["camille@example.com".into()]).await;
    let mut app2 = TestApp::with_state(strict);
    app2.set_cookies(app.cookies());
    assert_eq!(app2.get("/api/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_routes_are_rate_limited() {
    let (mut app, _google) = setup().await;
    let mut limited = None;
    for i in 0..40 {
        let r = app.get("/auth/google/start").await;
        if r.status == StatusCode::TOO_MANY_REQUESTS {
            limited = Some(i);
            break;
        }
    }
    assert_eq!(
        limited,
        Some(30),
        "the 31st request within a minute is refused"
    );
}

/// Throwaway RSA keys, generated per test run so that no private key is committed.
fn generate_pem() -> (String, String) {
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
    let key = rsa::RsaPrivateKey::new(&mut rand08::rngs::OsRng, 2048).unwrap();
    (
        key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string(),
        key.to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .unwrap(),
    )
}
