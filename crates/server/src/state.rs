//! Shared application state.

use std::ops::Deref;
use std::sync::Arc;

use annee_gcal::{CalendarStore, DemoStore};
use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use chrono::{NaiveDate, Utc};

use crate::config::{AppConfig, Mode};
use crate::google_login::GoogleOidc;
use crate::imports::education::{EducationSource, FixtureEducation};
use crate::imports::{ImportService, Importer};
use crate::oauth::OAuthService;
use crate::oauth::jwt::TokenKeys;
use crate::ratelimit::Limits;
use crate::storage::Db;
use crate::tokens::Tokens;

pub struct Inner {
    pub config: AppConfig,
    pub mode: Mode,
    pub store: Arc<dyn CalendarStore>,
    /// Present in demo mode only (reset endpoint).
    pub demo: Option<Arc<DemoStore>>,
    pub cookie_key: Key,
    /// `APP_FAKE_TODAY` (demo only).
    pub fake_today: Option<NaiveDate>,
    pub db: Db,
    pub imports: Arc<ImportService>,
    pub tokens: Tokens,
    pub oauth: Arc<OAuthService>,
    /// Google Sign-In (production only).
    pub oidc: Option<Arc<GoogleOidc>>,
    pub limits: Arc<Limits>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Key {
        state.cookie_key.clone()
    }
}

impl AppState {
    pub fn new(inner: Inner) -> Self {
        AppState(Arc::new(inner))
    }

    /// Demo state: in-memory store seeded with the prototype's data, in-memory SQLite and the
    /// recorded education fixtures.
    pub async fn demo(
        config: AppConfig,
        fake_today: Option<NaiveDate>,
        cookie_key: Key,
    ) -> anyhow::Result<Self> {
        Self::demo_with(config, fake_today, cookie_key, false).await
    }

    /// Like [`AppState::demo`]; `allow_insecure_cimd` (tests only) lets client metadata
    /// documents be fetched from `http://` / loopback URLs.
    pub async fn demo_with(
        config: AppConfig,
        fake_today: Option<NaiveDate>,
        cookie_key: Key,
        allow_insecure_cimd: bool,
    ) -> anyhow::Result<Self> {
        let base_url = config.app.base_url.clone();
        let demo = Arc::new(DemoStore::new(config.tz(), config.family.members.clone()));
        let db = Db::memory().await?;
        let imports = import_service(
            &config,
            demo.clone(),
            Arc::new(FixtureEducation),
            db.clone(),
            fake_today,
        );
        Ok(AppState::new(Inner {
            config,
            mode: Mode::Demo,
            store: demo.clone(),
            demo: Some(demo),
            cookie_key,
            fake_today,
            tokens: Tokens::new(db.clone()),
            oauth: OAuthService::new(
                db.clone(),
                TokenKeys::random(),
                base_url,
                allow_insecure_cimd,
            ),
            db,
            imports,
            oidc: None,
            limits: Limits::standard(),
        }))
    }

    /// "Today" in the configured time zone (fixed by `APP_FAKE_TODAY` in demo mode).
    pub fn today(&self) -> NaiveDate {
        self.fake_today
            .unwrap_or_else(|| Utc::now().with_timezone(&self.config.tz()).date_naive())
    }

    pub fn is_demo(&self) -> bool {
        self.mode == Mode::Demo
    }
}

/// "Today" for the import job: the fixed demo date, or the clock in the app's time zone.
pub fn import_service(
    config: &AppConfig,
    store: Arc<dyn CalendarStore>,
    education: Arc<dyn EducationSource>,
    db: Db,
    fake_today: Option<NaiveDate>,
) -> Arc<ImportService> {
    let tz = config.tz();
    let importer = Importer::new(store, education, config.imports.clone(), tz);
    ImportService::new(
        importer,
        db,
        Arc::new(move || fake_today.unwrap_or_else(|| Utc::now().with_timezone(&tz).date_naive())),
    )
}
