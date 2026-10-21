//! Command-line entry points other than `serve`.

use std::sync::Arc;

use annee_gcal::{CalendarStore, DemoStore, TypeRegistry};
use anyhow::anyhow;
use chrono::Utc;

use crate::bootstrap::{google_store, http_client, load_config};
use crate::config::{Env, Mode};
use crate::imports::education::{EducationSource, FixtureEducation, HttpEducation};
use crate::imports::{ImportService, Importer};
use crate::state::import_service;
use crate::storage::Db;
use crate::types_admin::{has_unfinished_migration, load_catalog};

/// `annee import [--dry-run]`.
pub async fn import(dry_run: bool, force_demo: bool) -> anyhow::Result<()> {
    let env = Env::from_process(force_demo).map_err(|e| anyhow!(e))?;
    let config = load_config(&env)?;
    env.require_prod_secrets(&["GOOGLE_SERVICE_ACCOUNT_FILE"])
        .map_err(|e| anyhow!(e))?;
    // A dry run has no side effect: it does not open (and so does not create or migrate) the
    // database, and plans with the default type colors.
    let db = if dry_run {
        None
    } else {
        Some(match env.mode {
            Mode::Demo => Db::memory().await?,
            Mode::Prod => Db::connect(&env.database_url).await?,
        })
    };
    // Imported events take the colors of the configured type catalog (§19).
    let types = Arc::new(match &db {
        Some(db) => TypeRegistry::new(load_catalog(db).await),
        None => TypeRegistry::default(),
    });
    let (store, education): (Arc<dyn CalendarStore>, Arc<dyn EducationSource>) = match env.mode {
        Mode::Demo => (
            Arc::new(
                DemoStore::new(config.tz(), config.family.members.clone())
                    .with_registry(types.clone()),
            ),
            Arc::new(FixtureEducation),
        ),
        Mode::Prod => (
            Arc::new(google_store(&env, &config, types.clone())?),
            Arc::new(HttpEducation::new(http_client()?)),
        ),
    };
    let tz = config.tz();
    let today = env
        .fake_today
        .unwrap_or_else(|| Utc::now().with_timezone(&tz).date_naive());

    let Some(db) = db else {
        let importer = Importer::new(store, education, config.imports.clone(), tz);
        let (counts, lines) = importer.run(today, true).await.map_err(|e| anyhow!(e))?;
        for l in lines {
            println!("{l}");
        }
        println!(
            "dry run: {} to create, {} to update, {} to delete, {} unchanged",
            counts.created, counts.updated, counts.deleted, counts.unchanged
        );
        return Ok(());
    };
    // The server may be rewriting colors (its in-memory flag is not visible from this process).
    if has_unfinished_migration(&db).await {
        return Err(anyhow!(
            "a category color change is unfinished: resume it in Réglages first"
        ));
    }

    let service: Arc<ImportService> =
        import_service(&config, store, education, db, env.fake_today, types);
    let run = service
        .run("cli")
        .await
        .ok_or_else(|| anyhow!("an import is already running"))?;
    match run.status.as_str() {
        "success" => {
            println!(
                "import finished: {} created, {} updated, {} deleted, {} unchanged",
                run.created.unwrap_or(0),
                run.updated.unwrap_or(0),
                run.deleted.unwrap_or(0),
                run.unchanged.unwrap_or(0)
            );
            Ok(())
        }
        _ => Err(anyhow!(
            "import failed: {}",
            run.error.unwrap_or_else(|| "unknown error".into())
        )),
    }
}

/// `annee check-config`: validate the configuration and secrets, then prove that both calendars
/// are reachable and writable (create + delete of a temporary event in each).
pub async fn check_config() -> anyhow::Result<()> {
    use annee_domain::CalendarKind;
    use annee_gcal::google::model::{GEvent, GTime};

    let env = Env::from_process(false).map_err(|e| anyhow!(e))?;
    let mut checks = Checks::default();
    let config = match load_config(&env) {
        Ok(c) => {
            checks.report(
                true,
                &format!("configuration {}", env.config_path.display()),
                String::new(),
            );
            c
        }
        Err(e) => {
            checks.report(false, "configuration", e.to_string());
            return Err(anyhow!("check-config failed"));
        }
    };
    if env.mode == Mode::Demo {
        println!("demo mode: no secrets or Google access to check");
        return Ok(());
    }
    match env.require_prod_secrets(&crate::config::SERVER_SECRETS) {
        Ok(()) => checks.report(true, "required environment variables", String::new()),
        Err(e) => checks.report(false, "required environment variables", e),
    }
    match crate::bootstrap::cookie_key(&env) {
        Ok(_) => checks.report(true, "SESSION_KEY", String::new()),
        Err(e) => checks.report(false, "SESSION_KEY", e.to_string()),
    }
    match crate::oauth::jwt::TokenKeys::from_base64(
        env.oauth_signing_key.as_deref().unwrap_or_default(),
    ) {
        Ok(_) => checks.report(true, "OAUTH_SIGNING_KEY", String::new()),
        Err(e) => checks.report(false, "OAUTH_SIGNING_KEY", e),
    }
    if !checks.failures.is_empty() {
        return Err(anyhow!(
            "check-config failed: {}",
            checks.failures.join(", ")
        ));
    }

    let store = match google_store(&env, &config, Arc::new(TypeRegistry::default())) {
        Ok(s) => {
            checks.report(true, "service account key", String::new());
            s
        }
        Err(e) => {
            checks.report(false, "service account key", e.to_string());
            return Err(anyhow!("check-config failed"));
        }
    };
    let client = store.client();
    let day = Utc::now().with_timezone(&config.tz()).date_naive() + chrono::Duration::days(2);
    for (cal, label) in [
        (CalendarKind::Reperes, "Repères"),
        (CalendarKind::Famille, "Famille"),
    ] {
        match client.calendar(cal).await {
            Ok(meta) => checks.report(
                true,
                &format!("calendar {label} readable"),
                format!("\"{}\"", meta.summary.unwrap_or_default()),
            ),
            Err(e) => {
                checks.report(false, &format!("calendar {label} readable"), format!("{e:?} (check the calendar id and that it is shared with the service account)"));
                continue;
            }
        }
        let probe = GEvent {
            summary: Some("annee check-config (temporary)".into()),
            start: Some(GTime::all_day(day.to_string())),
            end: Some(GTime::all_day(
                (day + chrono::Duration::days(1)).to_string(),
            )),
            ..Default::default()
        };
        match client.insert(cal, &probe).await {
            Ok(created) => {
                let id = created.id.unwrap_or_default();
                match client.delete(cal, &id).await {
                    Ok(()) => checks.report(
                        true,
                        &format!("calendar {label} writable"),
                        "temporary event created and deleted".into(),
                    ),
                    Err(e) => checks.report(
                        false,
                        &format!("calendar {label} writable"),
                        format!("created but could not delete {id}: {e:?}"),
                    ),
                }
            }
            Err(e) => checks.report(
                false,
                &format!("calendar {label} writable"),
                format!(
                    "{e:?} (share it with the service account with \"Make changes to events\")"
                ),
            ),
        }
    }
    if checks.failures.is_empty() {
        println!("all checks passed");
        Ok(())
    } else {
        Err(anyhow!(
            "check-config failed: {}",
            checks.failures.join(", ")
        ))
    }
}

#[derive(Default)]
struct Checks {
    failures: Vec<String>,
}

impl Checks {
    fn report(&mut self, ok: bool, what: &str, detail: String) {
        let tail = if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        };
        println!("{} {what}{tail}", if ok { "OK  " } else { "FAIL" });
        if !ok {
            self.failures.push(what.to_string());
        }
    }
}

/// `annee healthcheck`: GET `/healthz` on the local listener (the distroless image has no curl).
pub async fn healthcheck() -> anyhow::Result<()> {
    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?
        .get(format!("http://127.0.0.1:{port}/healthz"))
        .send()
        .await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!("healthz answered {}", resp.status()))
    }
}
