//! Administration of the type catalog (§19): persistence, preview and the resumable color
//! migration of Google events.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use annee_domain::catalog::{COLOR_NAMES, MAX_NAME};
use annee_domain::types::MAX_CUSTOM;
use annee_domain::{CatalogError, EventType, Labels, TypeCatalog, TypeDef};
use annee_gcal::{CalendarStore, StoreError, TypeRegistry};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::Mutex;

use crate::storage::{Db, now_ts};

const ATTEMPTS: u32 = 3;

/// One type of the catalog as exchanged with the Réglages page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeInput {
    pub code: String,
    #[serde(default)]
    pub color_id: Option<String>,
    pub labels: Labels,
}

/// Body of `PUT /api/admin/types`.
#[derive(Debug, Clone, Deserialize)]
pub struct TypesBody {
    pub types: Vec<TypeInput>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TypeChange {
    pub code: &'static str,
    pub from_color_id: Option<String>,
    pub to_color_id: Option<String>,
    /// Existing events whose color is rewritten.
    pub events: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub changes: Vec<TypeChange>,
    /// Existing events, of no type today, that take a type because it now owns their color.
    pub orphans: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum TypeAdminError {
    #[error("{0}")]
    Catalog(CatalogError),
    #[error("{0}")]
    Store(StoreError),
    /// A migration is running (or failed and must be resumed).
    #[error("migration in progress")]
    Busy,
    /// An unfinished migration targets another catalog.
    #[error("another migration is pending")]
    Pending,
    #[error("database error: {0}")]
    Database(String),
}

impl From<CatalogError> for TypeAdminError {
    fn from(e: CatalogError) -> Self {
        TypeAdminError::Catalog(e)
    }
}

impl From<StoreError> for TypeAdminError {
    fn from(e: StoreError) -> Self {
        TypeAdminError::Store(e)
    }
}

impl From<sqlx::Error> for TypeAdminError {
    fn from(e: sqlx::Error) -> Self {
        TypeAdminError::Database(e.to_string())
    }
}

type Mapping = BTreeMap<String, Option<String>>;

#[derive(Debug, Clone, Serialize)]
pub struct MigrationStatus {
    pub status: String,
    pub error: Option<String>,
    pub patched: Option<i64>,
}

struct Pending {
    id: String,
    target: TypeCatalog,
    mapping: Mapping,
}

pub struct TypeAdmin {
    db: Db,
    registry: Arc<TypeRegistry>,
    store: Arc<dyn CalendarStore>,
    /// Serializes `apply`/`resume`.
    lock: Mutex<()>,
    /// Pause between two attempts of a failing pass (multiplied by the attempt number).
    retry_delay: Duration,
}

/// Parses the wire format into a validated catalog.
pub fn catalog_from_input(types: Vec<TypeInput>) -> Result<TypeCatalog, CatalogError> {
    let mut defs = Vec::with_capacity(types.len());
    for t in types {
        let Some(kind) = EventType::from_code(&t.code) else {
            return Err(CatalogError::UnknownType(t.code));
        };
        defs.push(TypeDef {
            kind,
            color_id: t.color_id.filter(|c| !c.trim().is_empty()),
            labels: t.labels,
        });
    }
    TypeCatalog::new(defs)
}

fn row_to_def(r: &sqlx::sqlite::SqliteRow) -> Option<TypeDef> {
    let code: String = r.get("code");
    Some(TypeDef {
        kind: EventType::from_code(&code)?,
        color_id: r.get("color_id"),
        labels: Labels {
            fr: annee_domain::Names {
                singular: r.get("label_fr"),
                plural: r.get("label_plural_fr"),
            },
            en: annee_domain::Names {
                singular: r.get("label_en"),
                plural: r.get("label_plural_en"),
            },
        },
    })
}

/// The catalog stored in the database; an invalid or unreadable one falls back to the defaults.
pub async fn load_catalog(db: &Db) -> TypeCatalog {
    let rows = match sqlx::query("SELECT * FROM type_settings")
        .fetch_all(&db.pool)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "cannot read the type catalog, using the defaults");
            return TypeCatalog::default();
        }
    };
    let defs: Vec<TypeDef> = rows.iter().filter_map(row_to_def).collect();
    match TypeCatalog::with_overrides(defs) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "stored type catalog is invalid, using the defaults");
            TypeCatalog::default()
        }
    }
}

/// Whether a color migration is unfinished in the database (another process may be running it).
pub async fn has_unfinished_migration(db: &Db) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM type_migrations WHERE status != 'done'")
        .fetch_one(&db.pool)
        .await
        .map(|n| n > 0)
        .unwrap_or(false)
}

/// Wire form of a catalog for the editor.
fn editor_types(catalog: &TypeCatalog) -> Vec<Value> {
    catalog
        .defs()
        .iter()
        .map(|d| {
            json!({
                "code": d.kind.code(),
                "custom": d.kind.is_custom(),
                "color_id": d.color_id,
                "labels": d.labels,
            })
        })
        .collect()
}

impl TypeAdmin {
    pub fn new(db: Db, registry: Arc<TypeRegistry>, store: Arc<dyn CalendarStore>) -> Arc<Self> {
        Arc::new(TypeAdmin {
            db,
            registry,
            store,
            lock: Mutex::new(()),
            retry_delay: Duration::from_secs(2),
        })
    }

    /// Same admin with another retry pause (tests).
    pub fn with_retry_delay(self: Arc<Self>, retry_delay: Duration) -> Arc<Self> {
        match Arc::try_unwrap(self) {
            Ok(mut admin) => {
                admin.retry_delay = retry_delay;
                Arc::new(admin)
            }
            Err(shared) => shared,
        }
    }

    /// Demo only: forget every customization and go back to the default catalog.
    pub async fn reset(&self) -> Result<(), TypeAdminError> {
        let _guard = self.lock.lock().await;
        sqlx::query("DELETE FROM type_settings")
            .execute(&self.db.pool)
            .await?;
        sqlx::query("DELETE FROM type_migrations")
            .execute(&self.db.pool)
            .await?;
        self.registry.set(TypeCatalog::default());
        self.registry.set_migrating(false);
        self.store.invalidate_cache();
        Ok(())
    }

    /// Everything the Catégories section needs.
    pub async fn describe(&self) -> Result<Value, TypeAdminError> {
        let catalog = self.registry.current();
        let migration = self.last_unfinished().await?;
        Ok(json!({
            "palette": COLOR_NAMES
                .iter()
                .map(|(id, fr, en)| json!({"color_id": id, "name_fr": fr, "name_en": en}))
                .collect::<Vec<_>>(),
            "max_custom": MAX_CUSTOM,
            "max_name": MAX_NAME,
            "free_colors": catalog.free_colors(),
            "types": editor_types(&catalog),
            "migration": migration,
        }))
    }

    /// What saving `target` would rewrite in Google Calendar.
    pub async fn preview(&self, target: &TypeCatalog) -> Result<Preview, TypeAdminError> {
        let current = self.registry.current();
        let mapping = current.color_mapping(target);
        if mapping.is_empty() && target.free_colors() == current.free_colors() {
            return Ok(Preview {
                changes: vec![],
                orphans: 0,
            });
        }
        let counts = self.store.count_colors().await?;
        let count = |c: &Option<String>| {
            c.as_deref()
                .and_then(|c| counts.get(c))
                .copied()
                .unwrap_or(0)
        };
        let mut changes = Vec::new();
        for old in current.defs() {
            let to = target.get(old.kind).and_then(|d| d.color_id.clone());
            if old.color_id.is_some() && to != old.color_id {
                changes.push(TypeChange {
                    code: old.kind.code(),
                    from_color_id: old.color_id.clone(),
                    to_color_id: to,
                    events: count(&old.color_id),
                });
            }
        }
        // Colors that no type owns today but that a type owns afterwards.
        let owned_now: Vec<&str> = current
            .defs()
            .iter()
            .filter_map(|d| d.color_id.as_deref())
            .collect();
        let orphans = target
            .defs()
            .iter()
            .filter_map(|d| d.color_id.as_ref())
            .filter(|c| !owned_now.contains(&c.as_str()))
            .map(|c| counts.get(c).copied().unwrap_or(0))
            .sum();
        Ok(Preview { changes, orphans })
    }

    /// [`apply`](Self::apply) in a detached task: the migration goes on if the client that asked
    /// for it disconnects (a dropped handler would leave it half done and writes blocked).
    pub async fn apply_detached(
        self: &Arc<Self>,
        target: TypeCatalog,
    ) -> Result<(), TypeAdminError> {
        let this = self.clone();
        tokio::spawn(async move { this.apply(target).await })
            .await
            .map_err(|e| TypeAdminError::Database(format!("migration task: {e}")))?
    }

    /// [`resume`](Self::resume) in a detached task.
    pub async fn resume_detached(self: &Arc<Self>) -> Result<bool, TypeAdminError> {
        let this = self.clone();
        tokio::spawn(async move { this.resume().await })
            .await
            .map_err(|e| TypeAdminError::Database(format!("migration task: {e}")))?
    }

    /// Save `target`: names are applied at once, colors through the migration of §19.5.
    pub async fn apply(&self, target: TypeCatalog) -> Result<(), TypeAdminError> {
        let _guard = self.lock.try_lock().map_err(|_| TypeAdminError::Busy)?;
        let current = self.registry.current();
        let pending = self.pending().await?;
        // An unfinished migration comes first: only its own target (or `resume`) may go on.
        if let Some(p) = &pending
            && p.target != target
        {
            return Err(TypeAdminError::Pending);
        }
        if pending.is_none() {
            if *current == target {
                return Ok(());
            }
            if self.registry.is_migrating() {
                return Err(TypeAdminError::Busy);
            }
        }
        let mapping = current.color_mapping(&target);
        if mapping.is_empty() {
            self.persist(&target).await?;
            self.registry.set(target);
            self.store.invalidate_cache();
            return Ok(());
        }
        let id = match pending {
            Some(p) => p.id,
            None => self.record_migration(&target, &mapping).await?,
        };
        self.run_migration(&id, &mapping, &target).await
    }

    /// Resume the migration left unfinished by a crash or a failure; `Ok(false)` if none.
    pub async fn resume(&self) -> Result<bool, TypeAdminError> {
        let _guard = self.lock.try_lock().map_err(|_| TypeAdminError::Busy)?;
        let Some(p) = self.pending().await? else {
            self.registry.set_migrating(false);
            return Ok(false);
        };
        self.run_migration(&p.id, &p.mapping, &p.target).await?;
        Ok(true)
    }

    /// At startup: block writes right away when a migration is unfinished, then resume it.
    pub async fn resume_at_startup(self: &Arc<Self>) {
        match self.pending().await {
            Ok(Some(_)) => {
                self.registry.set_migrating(true);
                let this = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = this.resume().await {
                        tracing::error!(error = %e, "cannot resume the type migration");
                    }
                });
            }
            Ok(None) => {}
            Err(e) => tracing::error!(error = %e, "cannot read pending type migrations"),
        }
    }

    async fn run_migration(
        &self,
        id: &str,
        mapping: &Mapping,
        target: &TypeCatalog,
    ) -> Result<(), TypeAdminError> {
        // Blocks new writes and waits for the ones in flight; held until the pass is over.
        let _access = self.registry.begin_migration().await;
        let mut last_error = None;
        for attempt in 1..=ATTEMPTS {
            match self.store.recolor(id, mapping, target).await {
                Ok(patched) => {
                    self.persist(target).await?;
                    self.registry.set(target.clone());
                    self.store.invalidate_cache();
                    self.finish_migration(id, "done", Some(patched), None)
                        .await?;
                    self.registry.set_migrating(false);
                    tracing::info!(migration = id, patched, "type migration finished");
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!(migration = id, attempt, error = %e, "type migration step failed");
                    last_error = Some(e);
                    if attempt < ATTEMPTS {
                        tokio::time::sleep(self.retry_delay * attempt).await;
                    }
                }
            }
        }
        let e = last_error.unwrap_or(StoreError::Upstream("unknown".into()));
        // Events are partially rewritten: writes stay blocked until the migration is resumed.
        self.finish_migration(id, "failed", None, Some(e.to_string()))
            .await?;
        Err(TypeAdminError::Store(e))
    }

    async fn persist(&self, catalog: &TypeCatalog) -> Result<(), TypeAdminError> {
        let defaults = TypeCatalog::default();
        let mut tx = self.db.pool.begin().await?;
        sqlx::query("DELETE FROM type_settings")
            .execute(&mut *tx)
            .await?;
        for d in catalog.defs() {
            if defaults.get(d.kind) == Some(d) {
                continue;
            }
            sqlx::query(
                "INSERT INTO type_settings (code, color_id, label_fr, label_plural_fr, label_en, label_plural_en, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(d.kind.code())
            .bind(&d.color_id)
            .bind(&d.labels.fr.singular)
            .bind(&d.labels.fr.plural)
            .bind(&d.labels.en.singular)
            .bind(&d.labels.en.plural)
            .bind(now_ts())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn record_migration(
        &self,
        target: &TypeCatalog,
        mapping: &Mapping,
    ) -> Result<String, TypeAdminError> {
        let id = uuid::Uuid::new_v4().to_string();
        let target_json = serde_json::to_string(&editor_types(target)).unwrap_or_default();
        sqlx::query(
            "INSERT INTO type_migrations (id, started_at, status, target, mapping) VALUES (?, ?, 'running', ?, ?)",
        )
        .bind(&id)
        .bind(now_ts())
        .bind(target_json)
        .bind(serde_json::to_string(mapping).unwrap_or_default())
        .execute(&self.db.pool)
        .await?;
        Ok(id)
    }

    async fn finish_migration(
        &self,
        id: &str,
        status: &str,
        patched: Option<usize>,
        error: Option<String>,
    ) -> Result<(), TypeAdminError> {
        sqlx::query(
            "UPDATE type_migrations SET status = ?, finished_at = ?, patched = ?, error = ? WHERE id = ?",
        )
        .bind(status)
        .bind(now_ts())
        .bind(patched.map(|n| n as i64))
        .bind(error)
        .bind(id)
        .execute(&self.db.pool)
        .await?;
        Ok(())
    }

    async fn pending(&self) -> Result<Option<Pending>, TypeAdminError> {
        let row = sqlx::query(
            "SELECT id, target, mapping FROM type_migrations WHERE status != 'done' ORDER BY started_at DESC LIMIT 1",
        )
        .fetch_optional(&self.db.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let target: Vec<TypeInput> = serde_json::from_str(&row.get::<String, _>("target"))
            .map_err(|e| TypeAdminError::Database(e.to_string()))?;
        let mapping: Mapping = serde_json::from_str(&row.get::<String, _>("mapping"))
            .map_err(|e| TypeAdminError::Database(e.to_string()))?;
        Ok(Some(Pending {
            id: row.get("id"),
            target: catalog_from_input(target)?,
            mapping,
        }))
    }

    async fn last_unfinished(&self) -> Result<Option<MigrationStatus>, TypeAdminError> {
        let row = sqlx::query(
            "SELECT status, error, patched FROM type_migrations WHERE status != 'done' ORDER BY started_at DESC LIMIT 1",
        )
        .fetch_optional(&self.db.pool)
        .await?;
        Ok(row.map(|r| MigrationStatus {
            status: r.get("status"),
            error: r.get("error"),
            patched: r.get("patched"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annee_domain::{Lang, Names};
    use annee_gcal::DemoStore;

    async fn admin() -> (Arc<TypeAdmin>, Arc<TypeRegistry>, Arc<DemoStore>) {
        let db = Db::memory().await.unwrap();
        let registry = Arc::new(TypeRegistry::new(load_catalog(&db).await));
        let demo = Arc::new(
            DemoStore::new(chrono_tz::Europe::Paris, vec!["Alex".into()])
                .with_registry(registry.clone()),
        );
        let admin = TypeAdmin::new(db, registry.clone(), demo.clone());
        (admin, registry, demo)
    }

    fn customs(r: &TypeRegistry) -> usize {
        r.current()
            .defs()
            .iter()
            .filter(|d| d.kind.is_custom())
            .count()
    }

    fn names(s: &str) -> Names {
        Names {
            singular: s.into(),
            plural: format!("{s}s"),
        }
    }

    fn with_custom(base: &TypeCatalog, slot: u8, color: &str, name: &str) -> TypeCatalog {
        let mut defs = base.defs().to_vec();
        defs.push(TypeDef {
            kind: EventType::Custom(slot),
            color_id: Some(color.into()),
            labels: Labels {
                fr: names(name),
                en: names(name),
            },
        });
        TypeCatalog::new(defs).unwrap()
    }

    #[tokio::test]
    async fn rename_applies_without_migration_and_survives_reload() {
        let (admin, registry, _) = admin().await;
        let mut defs = registry.current().defs().to_vec();
        defs.iter_mut()
            .find(|d| d.kind == EventType::Trip)
            .unwrap()
            .labels
            .en
            .singular = "Holiday".into();
        let target = TypeCatalog::new(defs).unwrap();
        assert!(admin.preview(&target).await.unwrap().changes.is_empty());
        admin.apply(target).await.unwrap();
        assert_eq!(
            registry.current().label(EventType::Trip, Lang::En),
            "Holiday"
        );
        let reloaded = load_catalog(&admin.db).await;
        assert_eq!(reloaded.label(EventType::Trip, Lang::En), "Holiday");
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM type_settings")
            .fetch_one(&admin.db.pool)
            .await
            .unwrap();
        assert_eq!(n, 1, "only the changed type is stored");
    }

    #[tokio::test]
    async fn adding_then_deleting_a_custom_type_migrates_events() {
        let (admin, registry, demo) = admin().await;
        let target = with_custom(&registry.current(), 1, "6", "Sport");
        admin.apply(target).await.unwrap();
        assert_eq!(customs(&registry), 1);

        demo.list_year(2026).await.unwrap();
        // An event of the custom type.
        let new = annee_domain::NewEvent {
            kind: EventType::Custom(1),
            title: "Judo".into(),
            start: annee_domain::dates::ymd(2026, 4, 1),
            end: annee_domain::dates::ymd(2026, 4, 1),
            start_time: None,
            end_time: None,
            members: vec![],
            location: None,
            notes: None,
            birth_year: None,
        };
        demo.create(&new, "Alex").await.unwrap();

        let without: Vec<TypeDef> = registry
            .current()
            .defs()
            .iter()
            .filter(|d| d.kind != EventType::Custom(1))
            .cloned()
            .collect();
        let target = TypeCatalog::new(without).unwrap();
        let preview = admin.preview(&target).await.unwrap();
        assert_eq!(
            preview.changes,
            vec![TypeChange {
                code: "custom_1",
                from_color_id: Some("6".into()),
                to_color_id: None,
                events: 1
            }]
        );
        admin.apply(target).await.unwrap();
        assert_eq!(customs(&registry), 0);
        let evs = demo.list_year(2026).await.unwrap();
        assert_eq!(
            evs.iter().find(|e| e.title == "Judo").unwrap().kind,
            EventType::Unclassified
        );
        assert!(!registry.is_migrating());
        let status: String = sqlx::query_scalar("SELECT status FROM type_migrations")
            .fetch_one(&admin.db.pool)
            .await
            .unwrap();
        assert_eq!(status, "done");
    }

    #[tokio::test]
    async fn preview_counts_events_and_orphans() {
        let (admin, registry, demo) = admin().await;
        demo.list_year(2026).await.unwrap();
        let trips = demo
            .count_colors()
            .await
            .unwrap()
            .get("9")
            .copied()
            .unwrap_or(0);
        assert!(trips > 0);
        let mut defs = registry.current().defs().to_vec();
        defs.iter_mut()
            .find(|d| d.kind == EventType::Trip)
            .unwrap()
            .color_id = Some("6".into());
        let target = TypeCatalog::new(defs).unwrap();
        let p = admin.preview(&target).await.unwrap();
        assert_eq!(p.changes.len(), 1);
        assert_eq!(p.changes[0].events, trips);
        assert_eq!(p.orphans, 0);
    }

    #[tokio::test]
    async fn a_second_catalog_while_one_is_pending_is_refused() {
        let (admin, registry, _) = admin().await;
        let first = with_custom(&registry.current(), 1, "6", "Sport");
        admin
            .record_migration(&first, &registry.current().color_mapping(&first))
            .await
            .unwrap();
        registry.set_migrating(true);
        let other = with_custom(&registry.current(), 2, "7", "Musique");
        assert!(matches!(
            admin.apply(other).await,
            Err(TypeAdminError::Pending)
        ));
        // Resuming finishes the pending one.
        assert!(admin.resume().await.unwrap());
        assert_eq!(customs(&registry), 1);
        assert!(!registry.is_migrating());
    }

    fn recolored_trip(base: &TypeCatalog, color: &str) -> TypeCatalog {
        let mut defs = base.defs().to_vec();
        defs.iter_mut()
            .find(|d| d.kind == EventType::Trip)
            .unwrap()
            .color_id = Some(color.into());
        TypeCatalog::new(defs).unwrap()
    }

    async fn status(admin: &TypeAdmin) -> String {
        sqlx::query_scalar("SELECT status FROM type_migrations")
            .fetch_one(&admin.db.pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_failing_pass_keeps_writes_blocked_and_only_its_own_target_may_go_on() {
        let (admin, registry, demo) = admin().await;
        let admin = admin.with_retry_delay(Duration::ZERO);
        let old = registry.current();
        let target = recolored_trip(&old, "6");

        demo.set_fail_recolor(true);
        assert!(matches!(
            admin.apply(target.clone()).await,
            Err(TypeAdminError::Store(_))
        ));
        assert_eq!(status(&admin).await, "failed");
        assert!(registry.is_migrating(), "writes stay refused");
        assert!(registry.try_write_access().is_none());
        assert_eq!(*registry.current(), *old, "reads keep the old catalog");
        let described = admin.describe().await.unwrap();
        assert_eq!(described["migration"]["status"], "failed");
        assert!(described["migration"]["error"].is_string());

        // Re-submitting the old catalog (a "cancel") or another one is refused, not reported as done.
        assert!(matches!(
            admin.apply((*old).clone()).await,
            Err(TypeAdminError::Pending)
        ));
        assert!(matches!(
            admin.apply(recolored_trip(&old, "7")).await,
            Err(TypeAdminError::Pending)
        ));

        // Resuming while the outage lasts fails again; once it is over, the migration completes.
        assert!(admin.resume().await.is_err());
        demo.set_fail_recolor(false);
        assert!(admin.resume().await.unwrap());
        assert_eq!(*registry.current(), target);
        assert!(!registry.is_migrating());
        assert!(registry.try_write_access().is_some());
        assert_eq!(status(&admin).await, "done");
        assert!(!admin.resume().await.unwrap(), "nothing left to resume");
    }

    #[tokio::test]
    async fn an_unfinished_migration_is_resumed_at_startup() {
        let (admin, registry, _) = admin().await;
        let target = recolored_trip(&registry.current(), "6");
        let mapping = registry.current().color_mapping(&target);
        admin.record_migration(&target, &mapping).await.unwrap();
        assert!(has_unfinished_migration(&admin.db).await);

        admin.resume_at_startup().await;
        assert!(registry.is_migrating(), "writes are refused straight away");
        for _ in 0..200 {
            if !registry.is_migrating() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!registry.is_migrating());
        assert_eq!(*registry.current(), target);
        assert!(!has_unfinished_migration(&admin.db).await);
    }

    #[tokio::test]
    async fn the_detached_apply_survives_the_caller_being_dropped() {
        let (admin, registry, _) = admin().await;
        let target = recolored_trip(&registry.current(), "6");
        let caller = tokio::spawn({
            let admin = admin.clone();
            let target = target.clone();
            async move { admin.apply_detached(target).await }
        });
        // Let the request reach its await point, then drop it: the client went away.
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        caller.abort();
        for _ in 0..200 {
            if *registry.current() == target && !registry.is_migrating() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(*registry.current(), target);
        assert!(!registry.is_migrating());
        assert_eq!(status(&admin).await, "done");
    }
}
