//! Test harness: the real router over a demo state, driven in-process.
#![allow(dead_code)]

use std::collections::BTreeMap;

use std::sync::Arc;

use annee_gcal::DemoStore;
use annee_server::app;
use annee_server::config::{AppConfig, Mode};
use annee_server::google_login::{Endpoints, GoogleOidc};
use annee_server::imports::education::FixtureEducation;
use annee_server::ratelimit::Limits;
use annee_server::state::{AppState, Inner, import_service};
use annee_server::storage::Db;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum_extra::extract::cookie::Key;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

pub struct TestApp {
    pub state: AppState,
    router: Router,
    cookies: BTreeMap<String, String>,
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.text()))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

pub async fn demo_state() -> AppState {
    AppState::demo(
        AppConfig::default(),
        chrono::NaiveDate::from_ymd_opt(2026, 10, 16),
        Key::generate(),
    )
    .await
    .unwrap()
}

impl TestApp {
    pub async fn demo() -> Self {
        Self::with_state(demo_state().await)
    }

    pub fn with_state(state: AppState) -> Self {
        TestApp {
            router: app::router(state.clone()),
            state,
            cookies: BTreeMap::new(),
        }
    }

    pub fn cookies(&self) -> BTreeMap<String, String> {
        self.cookies.clone()
    }

    pub fn set_cookies(&mut self, cookies: BTreeMap<String, String>) {
        self.cookies = cookies;
    }

    pub fn clear_cookies(&mut self) {
        self.cookies.clear();
    }

    pub async fn send(&mut self, req: Request<Body>) -> Reply {
        let mut req = req;
        if !self.cookies.is_empty() && !req.headers().contains_key(header::COOKIE) {
            let cookie = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            req.headers_mut()
                .insert(header::COOKIE, cookie.parse().unwrap());
        }
        let res = self.router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let body = body.collect().await.unwrap().to_bytes().to_vec();
        for v in parts.headers.get_all(header::SET_COOKIE) {
            let s = v.to_str().unwrap();
            let (pair, rest) = s.split_once(';').unwrap_or((s, ""));
            let (name, value) = pair.split_once('=').unwrap();
            if value.is_empty() || rest.contains("Max-Age=0") {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_string(), value.to_string());
            }
        }
        Reply {
            status: parts.status,
            headers: parts.headers,
            body,
        }
    }

    pub async fn get(&mut self, uri: &str) -> Reply {
        self.send(Request::get(uri).body(Body::empty()).unwrap())
            .await
    }

    /// A mutating API call with the CSRF header.
    pub async fn api(&mut self, method: Method, uri: &str, body: Option<Value>) -> Reply {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-requested-with", "annee");
        let body = match body {
            Some(v) => {
                b = b.header(header::CONTENT_TYPE, "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        self.send(b.body(body).unwrap()).await
    }

    pub async fn post(&mut self, uri: &str, body: Value) -> Reply {
        self.api(Method::POST, uri, Some(body)).await
    }

    pub async fn patch(&mut self, uri: &str, body: Value) -> Reply {
        self.api(Method::PATCH, uri, Some(body)).await
    }

    pub async fn delete(&mut self, uri: &str) -> Reply {
        self.api(Method::DELETE, uri, None).await
    }

    /// Sign in with the demo login.
    pub async fn login(&mut self) {
        let r = self
            .send(Request::post("/auth/demo").body(Body::empty()).unwrap())
            .await;
        assert_eq!(
            r.status,
            StatusCode::SEE_OTHER,
            "demo login failed: {}",
            r.text()
        );
    }
}

/// A production-mode state (Google login enabled, allowlist, HTTPS base URL) over the demo
/// store, with Google's endpoints replaced by `endpoints`.
pub async fn prod_like_state(endpoints: Endpoints) -> AppState {
    prod_like_state_with(
        endpoints,
        Key::generate(),
        vec!["alex@example.com".into(), "Camille@Example.com".into()],
    )
    .await
}

pub async fn prod_like_state_with(
    endpoints: Endpoints,
    key: Key,
    allowed_emails: Vec<String>,
) -> AppState {
    let mut config = AppConfig::default();
    config.app.base_url = "https://famille.example.org".into();
    config.app.allowed_emails = allowed_emails;
    config
        .app
        .user_names
        .insert("alex@example.com".into(), "Alex".into());
    config.calendars.reperes_id = "reperes".into();
    config.calendars.famille_id = "famille".into();
    let types = Arc::new(annee_gcal::TypeRegistry::default());
    let store = Arc::new(
        DemoStore::new(config.tz(), config.family.members.clone()).with_registry(types.clone()),
    );
    let db = Db::memory().await.unwrap();
    let imports = import_service(
        &config,
        store.clone(),
        Arc::new(FixtureEducation),
        db.clone(),
        None,
        types.clone(),
    );
    let type_admin =
        annee_server::types_admin::TypeAdmin::new(db.clone(), types.clone(), store.clone());
    let oidc = Arc::new(GoogleOidc::new(
        reqwest::Client::new(),
        "client-id".into(),
        "client-secret".into(),
        endpoints,
    ));
    AppState::new(Inner {
        config,
        mode: Mode::Prod,
        store,
        demo: None,
        cookie_key: key,
        fake_today: None,
        tokens: annee_server::tokens::Tokens::new(db.clone()),
        oauth: annee_server::oauth::OAuthService::new(
            db.clone(),
            annee_server::oauth::jwt::TokenKeys::random(),
            "https://famille.example.org".into(),
            false,
        ),
        db,
        imports,
        oidc: Some(oidc),
        limits: Limits::standard(),
        types,
        type_admin,
    })
}

/// Serve the router of `state` on an ephemeral port; returns the base URL (`http://127.0.0.1:port`).
pub async fn spawn_server(state: AppState) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = app::router(state);
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}
