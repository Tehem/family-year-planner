//! Build the application state for the selected mode.

use std::sync::Arc;

use annee_gcal::google::auth::{ServiceAccountAuth, ServiceAccountKey};
use annee_gcal::{CalendarStore, GoogleConfig, GoogleStore, TypeRegistry};
use anyhow::{Context, anyhow};
use axum_extra::extract::cookie::Key;
use base64::Engine;

use crate::config::{AppConfig, Env, Mode, SERVER_SECRETS};
use crate::google_login::{Endpoints, GoogleOidc};
use crate::imports::education::HttpEducation;
use crate::oauth::OAuthService;
use crate::oauth::jwt::TokenKeys;
use crate::ratelimit::Limits;
use crate::state::{AppState, Inner, import_service};
use crate::storage::Db;
use crate::tokens::Tokens;
use crate::types_admin::{TypeAdmin, load_catalog};

pub fn load_config(env: &Env) -> anyhow::Result<AppConfig> {
    let config =
        AppConfig::load(&env.config_path, env.mode == Mode::Demo).map_err(|e| anyhow!(e))?;
    config
        .validate(env.mode)
        .map_err(|e| anyhow!("invalid configuration:\n{e}"))?;
    Ok(config)
}

pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("family-year-planner/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("cannot build the HTTP client")
}

pub fn cookie_key(env: &Env) -> anyhow::Result<Key> {
    match &env.session_key {
        Some(b64) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .context("SESSION_KEY must be base64")?;
            Key::try_from(bytes.as_slice())
                .map_err(|_| anyhow!("SESSION_KEY must decode to at least 64 bytes"))
        }
        None => Ok(Key::generate()),
    }
}

/// A Google store built from the service account key file.
pub fn google_store(
    env: &Env,
    config: &AppConfig,
    types: Arc<TypeRegistry>,
) -> anyhow::Result<GoogleStore> {
    let path = env.service_account_file.as_ref().ok_or_else(|| {
        anyhow!("missing required environment variable(s): GOOGLE_SERVICE_ACCOUNT_FILE")
    })?;
    let key = ServiceAccountKey::from_file(path).map_err(|e| anyhow!(e))?;
    let http = http_client()?;
    let auth = ServiceAccountAuth::new(key, http.clone()).map_err(|e| anyhow!(e))?;
    let cfg = GoogleConfig::new(
        config.calendars.reperes_id.clone(),
        config.calendars.famille_id.clone(),
        config.tz(),
        config.family.members.clone(),
    );
    Ok(GoogleStore::new(cfg, http, Arc::new(auth)).with_registry(types))
}

pub async fn build_state(env: &Env, config: AppConfig) -> anyhow::Result<AppState> {
    match env.mode {
        Mode::Demo => AppState::demo(config, env.fake_today, cookie_key(env)?).await,
        Mode::Prod => {
            env.require_prod_secrets(&SERVER_SECRETS)
                .map_err(|e| anyhow!(e))?;
            let base_url = config.app.base_url.clone();
            let keys = TokenKeys::from_base64(env.oauth_signing_key.as_deref().unwrap_or_default())
                .map_err(|e| anyhow!(e))?;
            let db = Db::connect(&env.database_url)
                .await
                .context("cannot open the database")?;
            let types = Arc::new(TypeRegistry::new(load_catalog(&db).await));
            let store: Arc<dyn CalendarStore> =
                Arc::new(google_store(env, &config, types.clone())?);
            let education = Arc::new(HttpEducation::new(http_client()?));
            let oidc = Arc::new(GoogleOidc::new(
                http_client()?,
                env.google_client_id.clone().unwrap_or_default(),
                env.google_client_secret.clone().unwrap_or_default(),
                Endpoints::default(),
            ));
            let imports = import_service(
                &config,
                store.clone(),
                education,
                db.clone(),
                None,
                types.clone(),
            );
            let type_admin = TypeAdmin::new(db.clone(), types.clone(), store.clone());
            Ok(AppState::new(Inner {
                config,
                mode: Mode::Prod,
                store,
                demo: None,
                cookie_key: cookie_key(env)?,
                fake_today: None,
                tokens: Tokens::new(db.clone()),
                oauth: OAuthService::new(db.clone(), keys, base_url, false),
                db,
                imports,
                oidc: Some(oidc),
                limits: Limits::standard(),
                types,
                type_admin,
            }))
        }
    }
}

/// Background work of a running server: import scheduler and startup catch-up.
pub async fn start_background(
    state: &AppState,
) -> anyhow::Result<Option<tokio_cron_scheduler::JobScheduler>> {
    crate::oauth::spawn_housekeeping(state.clone());
    state.imports.recover_interrupted().await?;
    state.type_admin.resume_at_startup().await;
    if state.is_demo() {
        return Ok(None); // demo data is seeded; imports are manual there
    }
    state.imports.run_if_stale().await;
    let sched = state
        .imports
        .start_scheduler(&state.config.imports.schedule, state.config.tz())
        .await?;
    Ok(Some(sched))
}
