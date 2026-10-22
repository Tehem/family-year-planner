//! Import runs: one at a time, recorded in SQLite, scheduled and triggerable on demand.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use annee_gcal::TypeRegistry;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use sqlx::Row;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

use super::{ImportCounts, Importer};
use crate::storage::{Db, now_ts};

/// `error` of a run that did not start because a category color migration was running.
pub const SKIPPED_MIGRATING: &str = "skipped:migrating";

/// Re-run at startup when the last success is older than this.
const STALE_AFTER_DAYS: i64 = 31;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportRun {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// `running`, `success` or `failed`.
    pub status: String,
    pub created: Option<i64>,
    pub updated: Option<i64>,
    pub deleted: Option<i64>,
    pub unchanged: Option<i64>,
    pub error: Option<String>,
    /// `schedule`, `startup`, `manual` or `cli`.
    pub trigger: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportStatus {
    pub running: bool,
    pub runs: Vec<ImportRun>,
}

pub type Clock = Arc<dyn Fn() -> NaiveDate + Send + Sync>;

pub struct ImportService {
    importer: Importer,
    db: Db,
    lock: Arc<Mutex<()>>,
    running: Arc<AtomicBool>,
    types: Arc<TypeRegistry>,
    today: Clock,
}

impl ImportService {
    pub fn new(importer: Importer, db: Db, types: Arc<TypeRegistry>, today: Clock) -> Arc<Self> {
        Arc::new(ImportService {
            importer,
            db,
            lock: Arc::new(Mutex::new(())),
            running: Arc::new(AtomicBool::new(false)),
            types,
            today,
        })
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.db.pool
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Runs left `running` by a crash cannot be resumed: mark them failed.
    pub async fn recover_interrupted(&self) -> anyhow::Result<()> {
        sqlx::query("UPDATE import_runs SET status='failed', finished_at=?, error='Interrompu (redémarrage du serveur).' WHERE status='running'")
            .bind(now_ts())
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    pub async fn status(&self) -> anyhow::Result<ImportStatus> {
        Ok(ImportStatus {
            running: self.is_running(),
            runs: self.recent_runs(10).await?,
        })
    }

    pub async fn recent_runs(&self, limit: i64) -> anyhow::Result<Vec<ImportRun>> {
        let rows = sqlx::query(
            "SELECT id, started_at, finished_at, status, created, updated, deleted, unchanged, error, trigger \
             FROM import_runs ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.db.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| ImportRun {
                id: r.get("id"),
                started_at: r.get("started_at"),
                finished_at: r.get("finished_at"),
                status: r.get("status"),
                created: r.get("created"),
                updated: r.get("updated"),
                deleted: r.get("deleted"),
                unchanged: r.get("unchanged"),
                error: r.get("error"),
                trigger: r.get("trigger"),
            })
            .collect())
    }

    pub async fn last_success(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        let t: Option<String> =
            sqlx::query_scalar("SELECT MAX(finished_at) FROM import_runs WHERE status='success'")
                .fetch_one(&self.db.pool)
                .await?;
        Ok(t.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)))
    }

    /// Run now and wait for the result. Returns `None` if another run is already in progress.
    pub async fn run(&self, trigger: &str) -> Option<ImportRun> {
        let guard = self.lock.clone().try_lock_owned().ok()?;
        self.running.store(true, Ordering::SeqCst);
        let run = self.run_locked(trigger).await;
        self.running.store(false, Ordering::SeqCst);
        drop(guard);
        run
    }

    /// Start a run in the background. `false` when one is already running.
    pub fn spawn(self: &Arc<Self>, trigger: &'static str) -> bool {
        let Ok(guard) = self.lock.clone().try_lock_owned() else {
            return false;
        };
        self.running.store(true, Ordering::SeqCst);
        let this = self.clone();
        tokio::spawn(async move {
            this.run_locked(trigger).await;
            this.running.store(false, Ordering::SeqCst);
            drop(guard);
        });
        true
    }

    async fn run_locked(&self, trigger: &str) -> Option<ImportRun> {
        // Held for the whole run: a color migration waits for it, and imports wait for a migration.
        let Some(_access) = self.types.try_write_access() else {
            tracing::warn!(
                trigger,
                "import skipped: the type colors are being migrated"
            );
            return self.record_skipped(trigger).await;
        };
        let id = match self.start_run(trigger).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(error = %e, "cannot record the import run");
                return None;
            }
        };
        let result = self.importer.run((self.today)(), false).await;
        match &result {
            Ok((counts, _)) => tracing::info!(?counts, trigger, "import finished"),
            Err(e) => tracing::error!(error = %e, trigger, "import failed"),
        }
        if let Err(e) = self
            .finish_run(id, result.map(|(c, _)| c).map_err(|e| e.to_string()))
            .await
        {
            tracing::error!(error = %e, "cannot record the end of the import run");
        }
        self.recent_runs(1).await.ok().and_then(|mut v| v.pop())
    }

    /// A run that could not start is still recorded, so that Réglages shows why nothing happened.
    async fn record_skipped(&self, trigger: &str) -> Option<ImportRun> {
        let id = self.start_run(trigger).await.ok()?;
        // A code, not a sentence: the page translates it ("skipped:" runs are not failures).
        self.finish_run(id, Err(SKIPPED_MIGRATING.to_string()))
            .await
            .ok()?;
        self.recent_runs(1).await.ok().and_then(|mut v| v.pop())
    }

    async fn start_run(&self, trigger: &str) -> anyhow::Result<i64> {
        let r = sqlx::query(
            "INSERT INTO import_runs (started_at, status, trigger) VALUES (?, 'running', ?)",
        )
        .bind(now_ts())
        .bind(trigger)
        .execute(&self.db.pool)
        .await?;
        Ok(r.last_insert_rowid())
    }

    async fn finish_run(
        &self,
        id: i64,
        result: Result<ImportCounts, String>,
    ) -> anyhow::Result<()> {
        match result {
            Ok(c) => {
                sqlx::query("UPDATE import_runs SET status='success', finished_at=?, created=?, updated=?, deleted=?, unchanged=? WHERE id=?")
                    .bind(now_ts())
                    .bind(c.created as i64)
                    .bind(c.updated as i64)
                    .bind(c.deleted as i64)
                    .bind(c.unchanged as i64)
                    .bind(id)
                    .execute(&self.db.pool)
                    .await?;
            }
            Err(msg) => {
                sqlx::query(
                    "UPDATE import_runs SET status='failed', finished_at=?, error=? WHERE id=?",
                )
                .bind(now_ts())
                .bind(msg)
                .bind(id)
                .execute(&self.db.pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Startup rule: run when there has been no successful run for over 31 days (or ever).
    pub async fn run_if_stale(self: &Arc<Self>) {
        let stale = match self.last_success().await {
            Ok(Some(t)) => Utc::now() - t > Duration::days(STALE_AFTER_DAYS),
            Ok(None) => true,
            Err(e) => {
                tracing::error!(error = %e, "cannot read the last import run");
                false
            }
        };
        if stale {
            self.spawn("startup");
        }
    }

    /// Schedule the job (`imports.schedule`, interpreted in the app's time zone).
    pub async fn start_scheduler(
        self: &Arc<Self>,
        cron: &str,
        tz: Tz,
    ) -> anyhow::Result<JobScheduler> {
        let sched = JobScheduler::new().await?;
        let this = self.clone();
        let job = Job::new_async_tz(cron, tz, move |_, _| {
            let this = this.clone();
            Box::pin(async move {
                if !this.spawn("schedule") {
                    tracing::info!("scheduled import skipped: one is already running");
                }
            })
        })?;
        sched.add(job).await?;
        sched.start().await?;
        Ok(sched)
    }
}
