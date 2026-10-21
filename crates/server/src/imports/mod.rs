//! Repères import job (§6): school holidays + public holidays → Repères calendar.

pub mod education;
pub mod holidays;
pub mod service;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use annee_domain::dates::{school_year_label, school_year_start, ymd};
use annee_domain::{Event, EventId};
use annee_gcal::{CalendarStore, ImportedEvent, StoreError};
use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use serde::Serialize;

use self::education::{EduError, EducationSource, school_events};
use crate::config::ImportsSection;

pub use service::{ImportRun, ImportService, ImportStatus};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ImportError {
    #[error("{0}")]
    Source(String),
    #[error("{0}")]
    Store(String),
}

impl From<EduError> for ImportError {
    fn from(e: EduError) -> Self {
        ImportError::Source(e.0)
    }
}

impl From<StoreError> for ImportError {
    fn from(e: StoreError) -> Self {
        ImportError::Store(match e {
            StoreError::Upstream(d) => format!("Google Calendar: {d}"),
            other => other.to_string(),
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ImportCounts {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
}

/// What a run would do.
#[derive(Debug, Default)]
pub struct ImportPlan {
    pub create: Vec<ImportedEvent>,
    pub update: Vec<(EventId, ImportedEvent)>,
    pub delete: Vec<Event>,
    pub unchanged: usize,
}

impl ImportPlan {
    pub fn counts(&self) -> ImportCounts {
        ImportCounts {
            created: self.create.len(),
            updated: self.update.len(),
            deleted: self.delete.len(),
            unchanged: self.unchanged,
        }
    }

    /// Human-readable lines for `annee import --dry-run`.
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for e in &self.create {
            lines.push(format!(
                "+ {} ({} → {}) [{}]",
                e.title, e.start, e.end, e.source_id
            ));
        }
        for (_, e) in &self.update {
            lines.push(format!(
                "~ {} ({} → {}) [{}]",
                e.title, e.start, e.end, e.source_id
            ));
        }
        for e in &self.delete {
            lines.push(format!(
                "- {} ({} → {}) [{}]",
                e.title,
                e.start,
                e.end,
                e.source_id.as_deref().unwrap_or("?")
            ));
        }
        lines
    }
}

pub struct Importer {
    store: Arc<dyn CalendarStore>,
    education: Arc<dyn EducationSource>,
    cfg: ImportsSection,
    tz: Tz,
}

/// School year (`2025-2026`) of a `edu:` source id.
fn edu_year(source_id: &str) -> Option<&str> {
    source_id.strip_prefix("edu:")?.split(':').next()
}

/// Calendar year of a `ferie:YYYY-MM-DD` source id.
fn ferie_year(source_id: &str) -> Option<i32> {
    source_id.strip_prefix("ferie:")?.get(..4)?.parse().ok()
}

impl Importer {
    pub fn new(
        store: Arc<dyn CalendarStore>,
        education: Arc<dyn EducationSource>,
        cfg: ImportsSection,
        tz: Tz,
    ) -> Self {
        Importer {
            store,
            education,
            cfg,
            tz,
        }
    }

    /// Compute the plan for `today` without writing anything.
    pub async fn plan(&self, today: NaiveDate) -> Result<ImportPlan, ImportError> {
        // 1. Desired events.
        let first_school_year = school_year_start(today);
        let mut desired: Vec<ImportedEvent> = Vec::new();
        let mut managed_school_years: HashSet<String> = HashSet::new();
        for i in 0..=self.cfg.school_years_ahead as i32 {
            let label = school_year_label(first_school_year + i);
            let records = self
                .education
                .fetch(&label, &self.cfg.school_location)
                .await?;
            let (events, skipped) = school_events(&label, &records, &self.cfg.school_zone, self.tz);
            tracing::info!(school_year = %label, events = events.len(), ?skipped, "school holidays fetched");
            if events.is_empty() {
                // Not published yet: leave whatever exists for that year untouched.
                continue;
            }
            managed_school_years.insert(label);
            desired.extend(events);
        }
        let first_ph_year = today.year();
        let last_ph_year = first_ph_year + self.cfg.public_holiday_years_ahead as i32;
        for year in first_ph_year..=last_ph_year {
            desired.extend(holidays::public_holiday_events(year));
        }

        // 2. Existing imported events in the window.
        let from = ymd(first_school_year, 9, 1).min(ymd(first_ph_year, 1, 1));
        let to = ymd(
            first_school_year + self.cfg.school_years_ahead as i32 + 1,
            9,
            30,
        )
        .max(ymd(last_ph_year, 12, 31));
        let existing = self.store.list_imported(from, to).await?;
        let mut by_sid: HashMap<String, Vec<Event>> = HashMap::new();
        for e in existing {
            if let Some(sid) = e.source_id.clone() {
                by_sid.entry(sid).or_default().push(e);
            }
        }

        // 3. Diff.
        let mut plan = ImportPlan::default();
        let desired_ids: HashSet<&str> = desired.iter().map(|d| d.source_id.as_str()).collect();
        for d in &desired {
            match by_sid.get(&d.source_id) {
                None => plan.create.push(d.clone()),
                Some(found) => {
                    let keep = &found[0];
                    plan.delete.extend(found[1..].iter().cloned()); // duplicates
                    if keep.title != d.title || keep.start != d.start || keep.end != d.end {
                        plan.update.push((keep.id.clone(), d.clone()));
                    } else {
                        plan.unchanged += 1;
                    }
                }
            }
        }
        for (sid, found) in &by_sid {
            if desired_ids.contains(sid.as_str()) {
                continue;
            }
            let managed = match (edu_year(sid), ferie_year(sid)) {
                (Some(y), _) => managed_school_years.contains(y),
                (_, Some(y)) => (first_ph_year..=last_ph_year).contains(&y),
                _ => false, // `demo:` and unknown ids are never touched
            };
            if managed {
                plan.delete.extend(found.iter().cloned());
            }
        }
        plan.create.sort_by_key(|e| (e.start, e.source_id.clone()));
        plan.update
            .sort_by_key(|(_, e)| (e.start, e.source_id.clone()));
        plan.delete.sort_by_key(|e| (e.start, e.id.to_string()));
        Ok(plan)
    }

    /// Apply a plan to the store.
    pub async fn apply(&self, plan: &ImportPlan) -> Result<ImportCounts, ImportError> {
        for e in &plan.create {
            self.store.create_imported(e).await?;
        }
        for (id, e) in &plan.update {
            self.store.update_imported(id, e).await?;
        }
        for e in &plan.delete {
            match self.store.delete_imported(&e.id).await {
                Ok(()) | Err(StoreError::NotFound) => {}
                Err(other) => return Err(other.into()),
            }
        }
        Ok(plan.counts())
    }

    pub async fn run(
        &self,
        today: NaiveDate,
        dry_run: bool,
    ) -> Result<(ImportCounts, Vec<String>), ImportError> {
        let plan = self.plan(today).await?;
        let lines = plan.describe();
        let counts = if dry_run {
            plan.counts()
        } else {
            self.apply(&plan).await?
        };
        Ok((counts, lines))
    }
}
