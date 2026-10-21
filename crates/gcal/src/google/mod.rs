//! `GoogleStore`: the `CalendarStore` backed by the Google Calendar REST API.

pub mod auth;
pub mod client;
pub mod mapping;
pub mod model;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use annee_domain::dates::local_midnight_utc;
use annee_domain::{CalendarKind, Event, EventId, EventType, NewEvent, Source, TypeCatalog};
use async_trait::async_trait;
use chrono::{Datelike, NaiveDate, SecondsFormat};
use chrono_tz::Tz;

use self::auth::TokenProvider;
use self::client::{ApiError, Client, ListParams};
use self::mapping::{DatePatch, MapCtx};
use self::model::GEvent;
use crate::recurrence::{birthday_series_start, instance_id, shift, yearly_occurrence};
use crate::registry::TypeRegistry;
use crate::store::{
    CalendarStore, DeleteScope, ImportedEvent, RecolorReport, StoreError, StoreResult,
};

pub const GOOGLE_API_BASE: &str = "https://www.googleapis.com";
const CACHE_TTL: Duration = Duration::from_secs(60);

pub struct GoogleConfig {
    pub api_base: String,
    pub reperes_id: String,
    pub famille_id: String,
    pub tz: Tz,
    pub members: Vec<String>,
    pub cache_ttl: Duration,
}

impl GoogleConfig {
    pub fn new(reperes_id: String, famille_id: String, tz: Tz, members: Vec<String>) -> Self {
        GoogleConfig {
            api_base: GOOGLE_API_BASE.to_string(),
            reperes_id,
            famille_id,
            tz,
            members,
            cache_ttl: CACHE_TTL,
        }
    }
}

type CachedYear = (Instant, Arc<Vec<Event>>);

pub struct GoogleStore {
    client: Client,
    tz: Tz,
    members: Vec<String>,
    registry: Arc<TypeRegistry>,
    ttl: Duration,
    cache: Mutex<HashMap<i32, CachedYear>>,
    /// Bumped by every invalidation: a read that started before it must not fill the cache.
    generation: AtomicU64,
}

impl GoogleStore {
    pub fn new(cfg: GoogleConfig, http: reqwest::Client, auth: Arc<dyn TokenProvider>) -> Self {
        GoogleStore {
            client: Client::new(http, cfg.api_base, auth, cfg.reperes_id, cfg.famille_id),
            tz: cfg.tz,
            members: cfg.members,
            registry: Arc::new(TypeRegistry::default()),
            ttl: cfg.cache_ttl,
            cache: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
        }
    }

    /// Use a shared, live type catalog (§19.6) instead of the default one.
    pub fn with_registry(mut self, registry: Arc<TypeRegistry>) -> Self {
        self.registry = registry;
        self
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    fn map(&self, g: &GEvent, cal: CalendarKind) -> Option<Event> {
        self.map_with(g, cal, &self.registry.current())
    }

    /// Maps with one catalog snapshot, so that a whole listing is typed consistently.
    fn map_with(&self, g: &GEvent, cal: CalendarKind, catalog: &TypeCatalog) -> Option<Event> {
        mapping::to_event(
            g,
            cal,
            &MapCtx {
                tz: self.tz,
                members: &self.members,
                catalog,
            },
        )
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.cache.lock().unwrap().clear();
    }

    fn rfc3339(&self, date: NaiveDate) -> String {
        local_midnight_utc(date, self.tz).to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    async fn list_calendar(
        &self,
        cal: CalendarKind,
        from: NaiveDate,
        to_exclusive: NaiveDate,
        catalog: &TypeCatalog,
    ) -> StoreResult<Vec<Event>> {
        let params = ListParams {
            time_min: Some(self.rfc3339(from)),
            time_max: Some(self.rfc3339(to_exclusive)),
            private_property: None,
            masters: false,
        };
        let raw = self.client.list(cal, &params).await?;
        Ok(raw
            .iter()
            .filter_map(|g| self.map_with(g, cal, catalog))
            .collect())
    }

    /// Fetch an event and, for an occurrence, its series master.
    async fn load(&self, id: &EventId) -> StoreResult<(GEvent, Option<GEvent>)> {
        let raw = self.client.get(id.calendar, &id.google_id).await?;
        if raw.status.as_deref() == Some("cancelled") {
            return Err(StoreError::NotFound);
        }
        let master = match raw.recurring_event_id.as_deref() {
            Some(master_id) => Some(self.client.get(id.calendar, master_id).await?),
            None => None,
        };
        Ok((raw, master))
    }

    async fn try_update(
        &self,
        id: &EventId,
        new: &NewEvent,
        writer: &str,
    ) -> Result<Event, ApiError> {
        let catalog = self.registry.current();
        let (raw, master) = self.load(id).await?;
        let current = self
            .map_with(&raw, id.calendar, &catalog)
            .ok_or(ApiError::NotFound)?;
        let target = master.as_ref().unwrap_or(&raw);
        let target_id = target.id.clone().ok_or(ApiError::NotFound)?;
        let master_event = master
            .as_ref()
            .and_then(|m| self.map_with(m, id.calendar, &catalog));
        let recurring = master.is_some() || raw.recurrence.is_some();

        // Which calendar must the event live in after the edit?
        let mut cal = id.calendar;
        let mut etag = target.etag.clone();
        let mut moved_from = None;
        if let Some(home) = new.kind.home_calendar()
            && new.kind != current.kind
            && home != cal
        {
            let moved = self.client.move_to(cal, &target_id, home).await?;
            moved_from = Some(cal);
            cal = home;
            etag = moved.etag.clone();
        }

        // Dates: only written when they (or the birth year of a birthday) changed.
        let dates_changed = new.start != current.start || new.end != current.end;
        let birth_changed = new.birth_year != current.birth_year;
        let dates = if dates_changed || (birth_changed && new.kind == EventType::Birthday) {
            if !recurring {
                Some(DatePatch {
                    start: new.start,
                    end_inclusive: new.end,
                    recurrence: None,
                })
            } else if new.kind == EventType::Birthday {
                let master_start = master_event.as_ref().map_or(current.start, |m| m.start);
                let start = birthday_series_start(new.start, new.birth_year, master_start);
                Some(DatePatch {
                    start,
                    end_inclusive: start,
                    recurrence: Some(vec![annee_domain::birthday::recurrence_rule(start)]),
                })
            } else {
                // Shift the whole series by the distance the occurrence moved.
                let (ms, me) = master_event
                    .as_ref()
                    .map_or((current.start, current.end), |m| (m.start, m.end));
                let delta = (new.start - current.start).num_days();
                let len = (new.end - new.start).num_days();
                let start = shift(ms, delta);
                let _ = me;
                Some(DatePatch {
                    start,
                    end_inclusive: start + chrono::Duration::days(len),
                    recurrence: None,
                })
            }
        } else {
            None
        };
        let include_color = new.kind != current.kind || moved_from.is_some();
        let color = catalog.color_id(new.kind);
        let body = mapping::patch_body(
            &current,
            new,
            writer,
            dates,
            (include_color && color.is_some()).then_some(color),
        );
        let patched = match self
            .client
            .patch(cal, &target_id, &body, etag.as_deref())
            .await
        {
            Err(ApiError::PreconditionFailed) => {
                // Re-read and retry once; a second failure is a conflict.
                let fresh = self.client.get(cal, &target_id).await?;
                self.client
                    .patch(cal, &target_id, &body, fresh.etag.as_deref())
                    .await?
            }
            other => other?,
        };
        self.invalidate();

        let mapped = self
            .map_with(&patched, cal, &catalog)
            .ok_or(ApiError::NotFound)?;
        if recurring || patched.recurrence.is_some() {
            // Return the occurrence the user was looking at.
            let date = yearly_occurrence(mapped.start, new.start.year()).unwrap_or(new.start);
            let iid = instance_id(&target_id, date);
            if let Ok(inst) = self.client.get(cal, &iid).await
                && let Some(e) = self.map_with(&inst, cal, &catalog)
            {
                return Ok(e);
            }
            return Ok(mapping::occurrence_of(&mapped, date));
        }
        Ok(mapped)
    }
}

#[async_trait]
impl CalendarStore for GoogleStore {
    fn timezone(&self) -> Tz {
        self.tz
    }

    async fn list_year(&self, year: i32) -> StoreResult<Vec<Event>> {
        if let Some((at, events)) = self.cache.lock().unwrap().get(&year)
            && at.elapsed() < self.ttl
        {
            return Ok(events.as_ref().clone());
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let catalog = self.registry.current();
        let from = annee_domain::dates::ymd(year, 1, 1);
        let to = annee_domain::dates::ymd(year + 1, 1, 1);
        let (reperes, famille) = tokio::join!(
            self.list_calendar(CalendarKind::Reperes, from, to, &catalog),
            self.list_calendar(CalendarKind::Famille, from, to, &catalog)
        );
        let mut events = reperes?;
        events.extend(famille?);
        events.sort_by(|a, b| {
            (a.start, a.end, a.id.to_string()).cmp(&(b.start, b.end, b.id.to_string()))
        });
        // A write or a catalog change during the fetch makes this listing stale: serve it, do not keep it.
        let mut cache = self.cache.lock().unwrap();
        if self.generation.load(Ordering::SeqCst) == generation {
            cache.insert(year, (Instant::now(), Arc::new(events.clone())));
        }
        Ok(events)
    }

    async fn get(&self, id: &EventId) -> StoreResult<Event> {
        let raw = self.client.get(id.calendar, &id.google_id).await?;
        self.map(&raw, id.calendar).ok_or(StoreError::NotFound)
    }

    async fn create(&self, new: &NewEvent, writer: &str) -> StoreResult<Event> {
        let cal = new
            .kind
            .home_calendar()
            .ok_or_else(|| StoreError::Upstream("type has no calendar".into()))?;
        let body = mapping::insert_body(new, writer, self.tz, &self.registry.current());
        let created = self.client.insert(cal, &body).await?;
        self.invalidate();
        let mapped = self
            .map(&created, cal)
            .ok_or_else(|| StoreError::Upstream("created event is unreadable".into()))?;
        if created.recurrence.is_some() {
            return Ok(mapping::occurrence_of(&mapped, new.start));
        }
        Ok(mapped)
    }

    async fn update(&self, id: &EventId, new: &NewEvent, writer: &str) -> StoreResult<Event> {
        Ok(self.try_update(id, new, writer).await?)
    }

    async fn delete(&self, id: &EventId) -> StoreResult<DeleteScope> {
        let raw = self.client.get(id.calendar, &id.google_id).await?;
        let (target, scope) = match raw.recurring_event_id {
            Some(master) => (master, DeleteScope::Series),
            None if raw.recurrence.is_some() => (id.google_id.clone(), DeleteScope::Series),
            None => (id.google_id.clone(), DeleteScope::Event),
        };
        self.client.delete(id.calendar, &target).await?;
        self.invalidate();
        Ok(scope)
    }

    async fn list_imported(&self, from: NaiveDate, to: NaiveDate) -> StoreResult<Vec<Event>> {
        let catalog = self.registry.current();
        let mut out = Vec::new();
        for src in [Source::ImportEducation, Source::ImportPublicHolidays] {
            let params = ListParams {
                time_min: Some(self.rfc3339(from)),
                time_max: Some(self.rfc3339(to + chrono::Duration::days(1))),
                private_property: Some(format!("{}={}", mapping::KEY_SOURCE, src.as_str())),
                masters: false,
            };
            let raw = self.client.list(CalendarKind::Reperes, &params).await?;
            out.extend(
                raw.iter()
                    .filter_map(|g| self.map_with(g, CalendarKind::Reperes, &catalog)),
            );
        }
        Ok(out)
    }

    async fn create_imported(&self, event: &ImportedEvent) -> StoreResult<()> {
        self.client
            .insert(
                CalendarKind::Reperes,
                &mapping::insert_imported_body(event, &self.registry.current()),
            )
            .await?;
        self.invalidate();
        Ok(())
    }

    async fn update_imported(&self, id: &EventId, event: &ImportedEvent) -> StoreResult<()> {
        self.client
            .patch(
                id.calendar,
                &id.google_id,
                &mapping::imported_patch(event, &self.registry.current()),
                None,
            )
            .await?;
        self.invalidate();
        Ok(())
    }

    async fn delete_imported(&self, id: &EventId) -> StoreResult<()> {
        self.client.delete(id.calendar, &id.google_id).await?;
        self.invalidate();
        Ok(())
    }

    fn invalidate_cache(&self) {
        self.invalidate();
    }

    async fn count_colors(&self) -> StoreResult<BTreeMap<String, usize>> {
        let (reperes, famille) = tokio::join!(
            self.client.list(CalendarKind::Reperes, &MASTERS),
            self.client.list(CalendarKind::Famille, &MASTERS)
        );
        let mut counts = BTreeMap::new();
        for g in reperes?.into_iter().chain(famille?) {
            if g.status.as_deref() == Some("cancelled") {
                continue;
            }
            if let Some(color) = g.color_id {
                *counts.entry(color).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    async fn recolor(
        &self,
        migration_id: &str,
        mapping: &BTreeMap<String, Option<String>>,
        _target: &TypeCatalog,
    ) -> StoreResult<RecolorReport> {
        let mut report = RecolorReport::default();
        let outcome = async {
            for cal in [CalendarKind::Reperes, CalendarKind::Famille] {
                for g in self.client.list(cal, &MASTERS).await? {
                    let (Some(id), Some(old)) = (g.id.as_deref(), g.color_id.as_deref()) else {
                        continue;
                    };
                    if g.status.as_deref() == Some("cancelled")
                        || g.private(mapping::KEY_MIGRATION) == Some(migration_id)
                    {
                        continue;
                    }
                    let Some(new) = mapping.get(old) else {
                        continue;
                    };
                    let body = mapping::recolor_patch(migration_id, new.as_deref());
                    match self.client.patch(cal, id, &body, None).await {
                        Ok(_) => report.patched += 1,
                        // Deleted meanwhile, or refused for good: leave this one, go on.
                        Err(e @ (ApiError::NotFound | ApiError::Rejected(_))) => {
                            tracing::warn!(event = id, error = ?e, "color migration skipped an event");
                            report.skipped += 1;
                        }
                        Err(other) => return Err(other.into()),
                    }
                }
            }
            Ok::<(), StoreError>(())
        }
        .await;
        // Whatever happened, events were rewritten: never serve stale types.
        self.invalidate();
        outcome.map(|()| report)
    }
}

/// List every event of a calendar once (series as masters), whatever its date.
const MASTERS: ListParams = ListParams {
    time_min: None,
    time_max: None,
    private_property: None,
    masters: true,
};
