//! The `CalendarStore` abstraction: everything Google-specific hides behind this trait so that the
//! in-memory demo store and the Google store are interchangeable.

use std::collections::BTreeMap;

use annee_domain::{Event, EventId, EventType, NewEvent, Source, TypeCatalog};
use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;

#[derive(Debug, Clone, thiserror::Error)]
pub enum StoreError {
    #[error("Événement introuvable.")]
    NotFound,
    #[error("Cet événement a été modifié entre-temps. Rechargez la page.")]
    Conflict,
    #[error("Google Agenda ne répond pas. Réessayez dans un instant.")]
    Upstream(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

/// What a deletion removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteScope {
    Event,
    Series,
}

impl DeleteScope {
    pub fn as_str(self) -> &'static str {
        match self {
            DeleteScope::Event => "event",
            DeleteScope::Series => "series",
        }
    }
}

/// An event written by the import job (read-only for users).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedEvent {
    /// `SchoolHoliday` or `PublicHoliday`.
    pub kind: EventType,
    pub title: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub source: Source,
    pub source_id: String,
}

#[async_trait]
pub trait CalendarStore: Send + Sync {
    fn timezone(&self) -> Tz;

    /// All events of both calendars overlapping the year (instances of recurring events
    /// expanded, cancelled events dropped).
    async fn list_year(&self, year: i32) -> StoreResult<Vec<Event>>;

    /// One event (an occurrence id resolves to the occurrence).
    async fn get(&self, id: &EventId) -> StoreResult<Event>;

    /// Create an event of `new.kind` in the calendar of its type.
    async fn create(&self, new: &NewEvent, writer: &str) -> StoreResult<Event>;

    /// Replace the user-editable fields of an event with `new` (patching only what changed).
    /// For an occurrence of a recurring event the whole series is edited.
    async fn update(&self, id: &EventId, new: &NewEvent, writer: &str) -> StoreResult<Event>;

    /// Delete an event, or the whole series for an occurrence.
    async fn delete(&self, id: &EventId) -> StoreResult<DeleteScope>;

    /// Imported events (`fy_source` starting with `import:`) overlapping `[from, to]`.
    async fn list_imported(&self, from: NaiveDate, to: NaiveDate) -> StoreResult<Vec<Event>>;
    async fn create_imported(&self, event: &ImportedEvent) -> StoreResult<()>;
    async fn update_imported(&self, id: &EventId, event: &ImportedEvent) -> StoreResult<()>;
    async fn delete_imported(&self, id: &EventId) -> StoreResult<()>;

    /// Number of events per Google `colorId` over both calendars and all years (a recurring
    /// series counts once). Events without a color are not counted. Used to preview §19.5.
    async fn count_colors(&self) -> StoreResult<BTreeMap<String, usize>>;

    /// Rewrite the color of existing events (§19.5): every event whose `colorId` is a key of
    /// `mapping` gets the mapped one (`None` clears it). One pass, so permutations are safe; events
    /// already tagged with `migration_id` are skipped, which makes the pass resumable. `target` is
    /// the catalog after the change (the demo store has no colors and re-derives types from it).
    /// Returns the number of events rewritten.
    async fn recolor(
        &self,
        migration_id: &str,
        mapping: &BTreeMap<String, Option<String>>,
        target: &TypeCatalog,
    ) -> StoreResult<usize>;

    /// Events overlapping `[from, to]`, sorted by start (then title), without duplicates.
    async fn list_range(&self, from: NaiveDate, to: NaiveDate) -> StoreResult<Vec<Event>> {
        let mut out: Vec<Event> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for year in from.year()..=to.year() {
            for e in self.list_year(year).await? {
                if e.overlaps(from, to) && seen.insert(e.id.clone()) {
                    out.push(e);
                }
            }
        }
        out.sort_by(|a, b| {
            (a.start, a.end, &a.title, a.id.to_string()).cmp(&(
                b.start,
                b.end,
                &b.title,
                b.id.to_string(),
            ))
        });
        Ok(out)
    }
}
