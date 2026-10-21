//! In-memory calendar store seeded with the prototype's sample data (`APP_MODE=demo`).

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use annee_domain::birthday::{age_at, first_occurrence};
use annee_domain::dates::{easter, public_holidays, year_bounds, ymd};
use annee_domain::{CalendarKind, Event, EventId, EventType, NewEvent, Source, TypeCatalog};
use async_trait::async_trait;
use chrono::{Datelike, Duration, NaiveDate};
use chrono_tz::Tz;

use crate::recurrence::{birthday_series_start, instance_id, shift, yearly_occurrence};
use crate::registry::TypeRegistry;
use crate::store::{CalendarStore, DeleteScope, ImportedEvent, StoreError, StoreResult};

/// Year in which demo birthday series start (any displayed year shows them).
const SERIES_START_YEAR: i32 = 1900;

/// Approximate school holidays of the prototype (mock, 2026 and 2027 only).
/// (title, (month, day) of the first day, (month, day) of the last day).
type SchoolRange = (&'static str, (u32, u32), (u32, u32));

const SCHOOL: [(i32, &[SchoolRange]); 2] = [
    (
        2026,
        &[
            ("Vacances de Noël", (1, 1), (1, 4)),
            ("Vacances d’hiver", (2, 21), (3, 8)),
            ("Vacances de printemps", (4, 18), (5, 3)),
            ("Pont de l’Ascension", (5, 14), (5, 17)),
            ("Vacances d’été", (7, 4), (8, 31)),
            ("Vacances de la Toussaint", (10, 17), (11, 1)),
            ("Vacances de Noël", (12, 19), (12, 31)),
        ],
    ),
    (
        2027,
        &[
            ("Vacances de Noël", (1, 1), (1, 3)),
            ("Vacances d’hiver", (2, 20), (3, 7)),
            ("Vacances de printemps", (4, 17), (5, 2)),
            ("Pont de l’Ascension", (5, 6), (5, 9)),
            ("Vacances d’été", (7, 3), (8, 31)),
            ("Vacances de la Toussaint", (10, 23), (11, 7)),
            ("Vacances de Noël", (12, 18), (12, 31)),
        ],
    ),
];

/// (title, type, first day, last day, members) of a sample event.
type SeedRow = (
    &'static str,
    EventType,
    NaiveDate,
    Option<NaiveDate>,
    Vec<String>,
);

#[derive(Debug, Clone)]
struct Stored {
    id: String,
    calendar: CalendarKind,
    kind: EventType,
    title: String,
    /// First day (for a yearly series: first occurrence).
    start: NaiveDate,
    end: NaiveDate,
    start_time: Option<chrono::NaiveTime>,
    end_time: Option<chrono::NaiveTime>,
    members: Vec<String>,
    location: Option<String>,
    notes: Option<String>,
    birth_year: Option<i32>,
    yearly: bool,
    source: Source,
    source_id: Option<String>,
    created_by: Option<String>,
    updated_by: Option<String>,
}

#[derive(Default)]
struct Inner {
    events: Vec<Stored>,
    seeded_years: HashSet<i32>,
    birthdays_seeded: bool,
    next_id: u64,
}

pub struct DemoStore {
    tz: Tz,
    members: Vec<String>,
    registry: Arc<TypeRegistry>,
    inner: Mutex<Inner>,
}

impl DemoStore {
    pub fn new(tz: Tz, members: Vec<String>) -> Self {
        DemoStore {
            tz,
            members,
            registry: Arc::new(TypeRegistry::default()),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Use a shared, live type catalog (§19.6) instead of the default one.
    pub fn with_registry(mut self, registry: Arc<TypeRegistry>) -> Self {
        self.registry = registry;
        self
    }

    /// Forget every change and start again from the sample data.
    pub fn reset(&self) {
        *self.inner.lock().unwrap() = Inner::default();
    }

    fn all_members(&self) -> Vec<String> {
        self.members.clone()
    }

    fn kids(&self) -> Vec<String> {
        // The prototype's "KIDS" are the last two members.
        let n = self.members.len();
        self.members[n.saturating_sub(2)..].to_vec()
    }

    fn pick(&self, names: &[&str]) -> Vec<String> {
        names
            .iter()
            .filter(|n| self.members.iter().any(|m| m == *n))
            .map(|n| n.to_string())
            .collect()
    }

    fn ensure_seeded(&self, inner: &mut Inner, year: i32) {
        if !inner.birthdays_seeded {
            inner.birthdays_seeded = true;
            for (title, m, d) in [
                ("Anniversaire de Mamie Odile", 1, 17),
                ("Anniversaire de Sacha", 3, 12),
                ("Anniversaire de Noé", 6, 3),
                ("Anniversaire de Camille", 9, 21),
                ("Anniversaire de Alex", 11, 28),
            ] {
                let start = ymd(SERIES_START_YEAR, m, d);
                self.push_seed(
                    inner,
                    EventType::Birthday,
                    title,
                    start,
                    start,
                    vec![],
                    true,
                    Source::Manual,
                    None,
                );
            }
        }
        if !inner.seeded_years.insert(year) {
            return;
        }
        // Public holidays (read-only, same source ids as the import job, so imports see them).
        for (date, title) in public_holidays(year) {
            let sid = format!("ferie:{date}");
            if inner
                .events
                .iter()
                .any(|e| e.source_id.as_deref() == Some(&sid))
            {
                continue;
            }
            self.push_seed(
                inner,
                EventType::PublicHoliday,
                title,
                date,
                date,
                vec![],
                false,
                Source::ImportPublicHolidays,
                Some(sid),
            );
        }
        // School holidays of the prototype: sample data, tagged `demo:` so that the import job
        // leaves them alone.
        if let Some((_, list)) = SCHOOL.iter().find(|(y, _)| *y == year) {
            for (i, (title, (sm, sd), (em, ed))) in list.iter().enumerate() {
                self.push_seed(
                    inner,
                    EventType::SchoolHoliday,
                    title,
                    ymd(year, *sm, *sd),
                    ymd(year, *em, *ed),
                    vec![],
                    false,
                    Source::ImportEducation,
                    Some(format!("demo:school:{year}:{i}")),
                );
            }
        }
        let d = |m, dd| ymd(year, m, dd);
        let e = easter(year);
        let day = Duration::days(1);
        let all = self.all_members();
        let kids = self.kids();
        let couple = self.pick(&["Alex", "Camille"]);
        let noé = self.pick(&["Noé"]);
        let norm = |m: Vec<String>| if m == all { vec![] } else { m };
        let rows: Vec<SeedRow> = vec![
            (
                "Baptême de la cousine Lou",
                EventType::Celebration,
                d(4, 26),
                None,
                all.clone(),
            ),
            (
                "Mariage de Julien et Sarah",
                EventType::Celebration,
                d(6, 13),
                Some(d(6, 14)),
                couple.clone(),
            ),
            (
                "Dîner des 40 ans de Marc",
                EventType::Celebration,
                d(10, 17),
                None,
                couple.clone(),
            ),
            (
                "Semaine de ski aux Arcs",
                EventType::Trip,
                d(2, 21),
                Some(d(2, 28)),
                all.clone(),
            ),
            (
                "Pâques chez les beaux-parents",
                EventType::FamilyHome,
                e - day,
                Some(e + day),
                all.clone(),
            ),
            (
                "Les enfants chez Mamie",
                EventType::FamilyHome,
                d(4, 20),
                Some(d(4, 24)),
                kids.clone(),
            ),
            (
                "Long week-end à Lisbonne",
                EventType::Trip,
                d(5, 14),
                Some(d(5, 17)),
                couple.clone(),
            ),
            (
                "Cousinade en Bretagne",
                EventType::FamilyGathering,
                d(7, 17),
                Some(d(7, 19)),
                all.clone(),
            ),
            (
                "Road trip en Italie",
                EventType::Trip,
                d(7, 25),
                Some(d(8, 2)),
                all.clone(),
            ),
            (
                "L'été à la maison de Normandie",
                EventType::FamilyHome,
                d(8, 3),
                Some(d(8, 16)),
                all.clone(),
            ),
            (
                "Crèche fermée",
                EventType::DaycareClosure,
                d(8, 3),
                Some(d(8, 24)),
                noé.clone(),
            ),
            (
                "Journée pédagogique de la crèche",
                EventType::DaycareClosure,
                d(3, 13),
                None,
                noé.clone(),
            ),
            (
                "Week-end des cousins",
                EventType::FamilyGathering,
                d(10, 3),
                Some(d(10, 4)),
                all.clone(),
            ),
            (
                "Noël chez Mamie Odile",
                EventType::FamilyHome,
                d(12, 23),
                Some(d(12, 26)),
                all.clone(),
            ),
            (
                "Crèche fermée",
                EventType::DaycareClosure,
                d(12, 24),
                Some(d(12, 31)),
                noé.clone(),
            ),
        ];
        for (title, kind, start, end, members) in rows {
            self.push_seed(
                inner,
                kind,
                title,
                start,
                end.unwrap_or(start),
                norm(members),
                false,
                Source::Manual,
                None,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_seed(
        &self,
        inner: &mut Inner,
        kind: EventType,
        title: &str,
        start: NaiveDate,
        end: NaiveDate,
        members: Vec<String>,
        yearly: bool,
        source: Source,
        source_id: Option<String>,
    ) {
        inner.next_id += 1;
        inner.events.push(Stored {
            id: format!("demo{}", inner.next_id),
            calendar: kind.home_calendar().unwrap_or(CalendarKind::Famille),
            kind,
            title: title.to_string(),
            start,
            end,
            start_time: None,
            end_time: None,
            members,
            location: None,
            notes: None,
            birth_year: None,
            yearly,
            source,
            source_id,
            created_by: None,
            updated_by: None,
        });
    }

    fn event_for(&self, st: &Stored, occurrence: Option<NaiveDate>) -> Event {
        let (id, series_id, start, end) = match occurrence {
            Some(occ) => (
                EventId::new(st.calendar, instance_id(&st.id, occ)),
                Some(EventId::new(st.calendar, st.id.clone())),
                occ,
                occ + (st.end - st.start),
            ),
            None => (
                EventId::new(st.calendar, st.id.clone()),
                None,
                st.start,
                st.end,
            ),
        };
        let age = if st.kind == EventType::Birthday {
            age_at(st.birth_year, start.year())
        } else {
            None
        };
        Event {
            id,
            series_id,
            calendar: st.calendar,
            kind: st.kind,
            title: st.title.clone(),
            start,
            end,
            start_time: st.start_time,
            end_time: st.end_time,
            members: st.members.clone(),
            location: st.location.clone(),
            notes: st.notes.clone(),
            birth_year: st.birth_year,
            age,
            source: st.source,
            read_only: st.source.is_import(),
            created_by: st.created_by.clone(),
            updated_by: st.updated_by.clone(),
            google_url: Some(format!(
                "https://calendar.google.com/calendar/event?demo={}",
                st.id
            )),
            source_id: st.source_id.clone(),
        }
    }

    fn expand(&self, st: &Stored, from: NaiveDate, to: NaiveDate, out: &mut Vec<Event>) {
        if st.yearly {
            let span = st.end - st.start;
            // Occurrences of the years touching the window (an occurrence can spill over).
            for y in (from.year() - 1)..=to.year() {
                if let Some(occ) = yearly_occurrence(st.start, y)
                    && occ <= to
                    && occ + span >= from
                {
                    out.push(self.event_for(st, Some(occ)));
                }
            }
        } else if st.start <= to && st.end >= from {
            out.push(self.event_for(st, None));
        }
    }

    fn find<'a>(&self, inner: &'a Inner, id: &EventId) -> Option<(&'a Stored, Option<NaiveDate>)> {
        if let Some(st) = inner
            .events
            .iter()
            .find(|e| e.calendar == id.calendar && e.id == id.google_id)
        {
            return Some((st, None));
        }
        let (master, suffix) = id.google_id.rsplit_once('_')?;
        let date = NaiveDate::parse_from_str(suffix, "%Y%m%d").ok()?;
        let st = inner
            .events
            .iter()
            .find(|e| e.calendar == id.calendar && e.id == master && e.yearly)?;
        (yearly_occurrence(st.start, date.year()) == Some(date)).then_some((st, Some(date)))
    }
}

#[async_trait]
impl CalendarStore for DemoStore {
    fn timezone(&self) -> Tz {
        self.tz
    }

    async fn list_year(&self, year: i32) -> StoreResult<Vec<Event>> {
        let mut inner = self.inner.lock().unwrap();
        self.ensure_seeded(&mut inner, year);
        let (from, to) = year_bounds(year);
        let mut out = Vec::new();
        for st in &inner.events {
            self.expand(st, from, to, &mut out);
        }
        out.sort_by(|a, b| {
            (a.start, a.end, a.id.to_string()).cmp(&(b.start, b.end, b.id.to_string()))
        });
        Ok(out)
    }

    async fn get(&self, id: &EventId) -> StoreResult<Event> {
        let inner = self.inner.lock().unwrap();
        let (st, occ) = self.find(&inner, id).ok_or(StoreError::NotFound)?;
        Ok(self.event_for(st, occ.or(st.yearly.then_some(st.start))))
    }

    async fn create(&self, new: &NewEvent, writer: &str) -> StoreResult<Event> {
        let mut inner = self.inner.lock().unwrap();
        inner.next_id += 1;
        let calendar = new.kind.home_calendar().unwrap_or(CalendarKind::Famille);
        let yearly = new.kind == EventType::Birthday;
        let start = if yearly {
            first_occurrence(new.start, new.birth_year).unwrap_or(new.start)
        } else {
            new.start
        };
        let st = Stored {
            id: format!("demo{}", inner.next_id),
            calendar,
            kind: new.kind,
            title: new.title.clone(),
            start,
            end: start + (new.end - new.start),
            start_time: None,
            end_time: None,
            members: new.members.clone(),
            location: new.location.clone(),
            notes: new.notes.clone(),
            birth_year: new.birth_year,
            yearly,
            source: Source::Manual,
            source_id: None,
            created_by: Some(writer.to_string()),
            updated_by: Some(writer.to_string()),
        };
        let ev = self.event_for(&st, yearly.then_some(new.start));
        inner.events.push(st);
        Ok(ev)
    }

    async fn update(&self, id: &EventId, new: &NewEvent, writer: &str) -> StoreResult<Event> {
        let mut inner = self.inner.lock().unwrap();
        let (existing, occ) = {
            let (st, occ) = self.find(&inner, id).ok_or(StoreError::NotFound)?;
            (st.clone(), occ)
        };
        let current = self.event_for(&existing, occ.or(existing.yearly.then_some(existing.start)));
        let mut st = existing;
        st.title = new.title.clone();
        st.kind = new.kind;
        st.members = new.members.clone();
        st.location = new.location.clone();
        st.notes = new.notes.clone();
        st.birth_year = new.birth_year;
        st.updated_by = Some(writer.to_string());
        if let Some(home) = new.kind.home_calendar() {
            st.calendar = home;
        }
        let dates_changed = new.start != current.start || new.end != current.end;
        if dates_changed || new.birth_year != current.birth_year {
            if st.yearly && new.kind == EventType::Birthday {
                st.start = birthday_series_start(new.start, new.birth_year, st.start);
                st.end = st.start;
            } else if st.yearly {
                let delta = (new.start - current.start).num_days();
                st.start = shift(st.start, delta);
                st.end = st.start + (new.end - new.start);
            } else {
                st.start = new.start;
                st.end = new.end;
                st.start_time = None;
                st.end_time = None;
            }
        }
        let pos = inner
            .events
            .iter()
            .position(|e| e.id == st.id)
            .ok_or(StoreError::NotFound)?;
        let occurrence = st
            .yearly
            .then(|| yearly_occurrence(st.start, new.start.year()).unwrap_or(st.start));
        let ev = self.event_for(&st, occurrence);
        inner.events[pos] = st;
        Ok(ev)
    }

    async fn delete(&self, id: &EventId) -> StoreResult<DeleteScope> {
        let mut inner = self.inner.lock().unwrap();
        let (st, _) = self.find(&inner, id).ok_or(StoreError::NotFound)?;
        let (master, scope) = (
            st.id.clone(),
            if st.yearly {
                DeleteScope::Series
            } else {
                DeleteScope::Event
            },
        );
        inner.events.retain(|e| e.id != master);
        Ok(scope)
    }

    async fn list_imported(&self, from: NaiveDate, to: NaiveDate) -> StoreResult<Vec<Event>> {
        let mut inner = self.inner.lock().unwrap();
        for y in from.year()..=to.year() {
            self.ensure_seeded(&mut inner, y);
        }
        let mut out = Vec::new();
        for st in inner.events.iter().filter(|e| e.source.is_import()) {
            // `demo:` events are sample data that imports never touch.
            if st
                .source_id
                .as_deref()
                .is_some_and(|s| s.starts_with("demo:"))
            {
                continue;
            }
            self.expand(st, from, to, &mut out);
        }
        Ok(out)
    }

    async fn create_imported(&self, event: &ImportedEvent) -> StoreResult<()> {
        let mut inner = self.inner.lock().unwrap();
        self.push_seed(
            &mut inner,
            event.kind,
            &event.title,
            event.start,
            event.end,
            vec![],
            false,
            event.source,
            Some(event.source_id.clone()),
        );
        Ok(())
    }

    async fn update_imported(&self, id: &EventId, event: &ImportedEvent) -> StoreResult<()> {
        let mut inner = self.inner.lock().unwrap();
        let st = inner
            .events
            .iter_mut()
            .find(|e| e.calendar == id.calendar && e.id == id.google_id)
            .ok_or(StoreError::NotFound)?;
        st.title = event.title.clone();
        st.start = event.start;
        st.end = event.end;
        Ok(())
    }

    async fn delete_imported(&self, id: &EventId) -> StoreResult<()> {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.events.len();
        inner
            .events
            .retain(|e| !(e.calendar == id.calendar && e.id == id.google_id));
        if inner.events.len() == before {
            Err(StoreError::NotFound)
        } else {
            Ok(())
        }
    }

    async fn count_colors(&self) -> StoreResult<BTreeMap<String, usize>> {
        let catalog = self.registry.current();
        let inner = self.inner.lock().unwrap();
        let mut counts = BTreeMap::new();
        for st in &inner.events {
            if let Some(color) = catalog.color_id(st.kind) {
                *counts.entry(color.to_string()).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    async fn recolor(
        &self,
        _migration_id: &str,
        mapping: &BTreeMap<String, Option<String>>,
        target: &TypeCatalog,
    ) -> StoreResult<usize> {
        let current = self.registry.current();
        let mut inner = self.inner.lock().unwrap();
        let mut patched = 0;
        for st in &mut inner.events {
            let Some(old) = current.color_id(st.kind) else {
                continue;
            };
            let Some(new) = mapping.get(old) else {
                continue;
            };
            st.kind = target.from_color_id(new.as_deref());
            if let Some(home) = st.kind.home_calendar() {
                st.calendar = home;
            }
            patched += 1;
        }
        Ok(patched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annee_domain::{EventInput, ValidationContext};

    fn store() -> DemoStore {
        DemoStore::new(
            chrono_tz::Europe::Paris,
            ["Alex", "Camille", "Sacha", "Noé"]
                .map(String::from)
                .to_vec(),
        )
    }

    fn ctx(m: &[String]) -> ValidationContext<'_> {
        static CATALOG: std::sync::LazyLock<TypeCatalog> =
            std::sync::LazyLock::new(TypeCatalog::default);
        ValidationContext {
            members: m,
            today: ymd(2026, 10, 16),
            catalog: &CATALOG,
        }
    }

    fn new_event(kind: &str, title: &str, start: &str, end: &str) -> NewEvent {
        let s = store();
        annee_domain::validation::validate_new(
            &EventInput {
                kind: Some(kind.into()),
                title: Some(title.into()),
                start: Some(start.into()),
                end: Some(end.into()),
                ..Default::default()
            },
            &ctx(&s.members),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn seeds_the_prototype_data() {
        let s = store();
        let evs = s.list_year(2026).await.unwrap();
        let count = |k: EventType| evs.iter().filter(|e| e.kind == k).count();
        assert_eq!(count(EventType::Birthday), 5);
        assert_eq!(count(EventType::PublicHoliday), 11);
        assert_eq!(count(EventType::SchoolHoliday), 7);
        assert_eq!(count(EventType::Celebration), 3);
        assert_eq!(count(EventType::DaycareClosure), 3);
        assert_eq!(count(EventType::FamilyHome), 4);
        assert_eq!(count(EventType::Trip), 3);
        assert_eq!(count(EventType::FamilyGathering), 2);
        let sacha = evs
            .iter()
            .find(|e| e.title == "Anniversaire de Sacha")
            .unwrap();
        assert_eq!(sacha.start, ymd(2026, 3, 12));
        assert!(sacha.series_id.is_some());
        assert_eq!(sacha.calendar, CalendarKind::Reperes);
        let easter = evs.iter().find(|e| e.title.starts_with("Pâques")).unwrap();
        assert_eq!(
            (easter.start, easter.end),
            (ymd(2026, 4, 4), ymd(2026, 4, 6))
        );
        // The prototype only has school holidays for 2026 and 2027.
        assert_eq!(
            s.list_year(2028)
                .await
                .unwrap()
                .iter()
                .filter(|e| e.kind == EventType::SchoolHoliday)
                .count(),
            0
        );
        assert_eq!(
            s.list_year(2028)
                .await
                .unwrap()
                .iter()
                .filter(|e| e.kind == EventType::Birthday)
                .count(),
            5
        );
    }

    #[tokio::test]
    async fn imported_types_are_read_only() {
        let s = store();
        let evs = s.list_year(2026).await.unwrap();
        for e in evs
            .iter()
            .filter(|e| matches!(e.kind, EventType::SchoolHoliday | EventType::PublicHoliday))
        {
            assert!(e.read_only, "{}", e.title);
        }
        assert!(
            evs.iter()
                .filter(|e| e.kind == EventType::Trip)
                .all(|e| !e.read_only)
        );
    }

    #[tokio::test]
    async fn create_get_update_delete() {
        let s = store();
        let created = s
            .create(
                &new_event("trip", "Bretagne", "2026-08-10", "2026-08-12"),
                "Alex",
            )
            .await
            .unwrap();
        assert_eq!(created.calendar, CalendarKind::Famille);
        assert_eq!(created.created_by.as_deref(), Some("Alex"));
        assert!(
            s.list_year(2026)
                .await
                .unwrap()
                .iter()
                .any(|e| e.id == created.id)
        );

        let mut changed = new_event("family_home", "Normandie", "2026-08-11", "2026-08-14");
        changed.members = vec!["Camille".into()];
        let updated = s.update(&created.id, &changed, "Camille").await.unwrap();
        assert_eq!(updated.kind, EventType::FamilyHome);
        assert_eq!(updated.updated_by.as_deref(), Some("Camille"));
        assert_eq!(updated.created_by.as_deref(), Some("Alex"));
        assert_eq!(
            (updated.start, updated.end),
            (ymd(2026, 8, 11), ymd(2026, 8, 14))
        );
        assert_eq!(s.get(&updated.id).await.unwrap().title, "Normandie");

        assert_eq!(s.delete(&created.id).await.unwrap(), DeleteScope::Event);
        assert!(matches!(
            s.get(&created.id).await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            s.delete(&created.id).await,
            Err(StoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn birthdays_are_yearly_series_with_ages() {
        let s = store();
        let mut nb = new_event(
            "birthday",
            "Anniversaire de Léa",
            "2026-05-05",
            "2026-05-05",
        );
        nb.birth_year = Some(2020);
        let created = s.create(&nb, "Alex").await.unwrap();
        assert_eq!(created.age, Some(6));
        let master = created.series_id.clone().unwrap();
        for (year, age) in [(2025, Some(5)), (2026, Some(6)), (2030, Some(10))] {
            let evs = s.list_year(year).await.unwrap();
            let e = evs
                .iter()
                .find(|e| e.title == "Anniversaire de Léa")
                .unwrap();
            assert_eq!(e.start, ymd(year, 5, 5));
            assert_eq!(e.age, age);
            assert_eq!(e.series_id.as_ref(), Some(&master));
        }
        assert!(
            s.list_year(2019)
                .await
                .unwrap()
                .iter()
                .all(|e| e.title != "Anniversaire de Léa")
        );

        // Deleting an occurrence deletes the series.
        let occ = s
            .list_year(2028)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.title == "Anniversaire de Léa")
            .unwrap();
        assert_eq!(s.delete(&occ.id).await.unwrap(), DeleteScope::Series);
        assert!(
            s.list_year(2026)
                .await
                .unwrap()
                .iter()
                .all(|e| e.title != "Anniversaire de Léa")
        );
    }

    #[tokio::test]
    async fn editing_an_occurrence_edits_the_series() {
        let s = store();
        let sacha = s
            .list_year(2027)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.title == "Anniversaire de Sacha")
            .unwrap();
        let mut n = new_event(
            "birthday",
            "Anniversaire de Sacha",
            "2027-03-13",
            "2027-03-13",
        );
        n.birth_year = Some(2018);
        let updated = s.update(&sacha.id, &n, "Alex").await.unwrap();
        assert_eq!(updated.start, ymd(2027, 3, 13));
        assert_eq!(updated.age, Some(9));
        let e2026 = s
            .list_year(2026)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.title == "Anniversaire de Sacha")
            .unwrap();
        assert_eq!(e2026.start, ymd(2026, 3, 13));
        assert_eq!(e2026.age, Some(8));
    }

    #[tokio::test]
    async fn leap_day_birthday_moves_to_28_february() {
        let s = store();
        let mut n = new_event("birthday", "Bébé bissextile", "2028-02-29", "2028-02-29");
        n.birth_year = Some(2020);
        s.create(&n, "Alex").await.unwrap();
        let find = |evs: Vec<Event>| {
            evs.into_iter()
                .find(|e| e.title == "Bébé bissextile")
                .unwrap()
        };
        assert_eq!(
            find(s.list_year(2028).await.unwrap()).start,
            ymd(2028, 2, 29)
        );
        assert_eq!(
            find(s.list_year(2027).await.unwrap()).start,
            ymd(2027, 2, 28)
        );
    }

    #[tokio::test]
    async fn imports_see_public_holidays_but_not_demo_school_holidays() {
        let s = store();
        let imported = s
            .list_imported(ymd(2026, 1, 1), ymd(2026, 12, 31))
            .await
            .unwrap();
        assert_eq!(imported.len(), 11, "only the public holidays");
        assert!(
            imported
                .iter()
                .all(|e| e.source_id.as_deref().unwrap().starts_with("ferie:"))
        );
        // Seeding a year after importing into it does not duplicate holidays.
        let before = s
            .list_year(2029)
            .await
            .unwrap()
            .iter()
            .filter(|e| e.kind == EventType::PublicHoliday)
            .count();
        assert_eq!(before, 11);
    }

    #[tokio::test]
    async fn reset_restores_the_sample_data() {
        let s = store();
        s.create(&new_event("trip", "X", "2026-08-10", "2026-08-12"), "Alex")
            .await
            .unwrap();
        s.reset();
        assert!(
            s.list_year(2026)
                .await
                .unwrap()
                .iter()
                .all(|e| e.title != "X")
        );
    }

    #[tokio::test]
    async fn list_range_spans_years_without_duplicates() {
        let s = store();
        let evs = s
            .list_range(ymd(2026, 12, 20), ymd(2027, 1, 5))
            .await
            .unwrap();
        let mut ids: Vec<String> = evs.iter().map(|e| e.id.to_string()).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(evs.iter().any(|e| e.title == "Noël"));
        assert!(evs.windows(2).all(|w| w[0].start <= w[1].start));
    }

    #[tokio::test]
    async fn recolor_swaps_and_clears_types() {
        use annee_domain::{Labels, Names, TypeDef};
        let registry = Arc::new(TypeRegistry::default());
        let s = DemoStore::new(
            chrono_tz::Europe::Paris,
            ["Alex", "Camille", "Sacha", "Noé"]
                .map(String::from)
                .to_vec(),
        )
        .with_registry(registry.clone());
        s.list_year(2026).await.unwrap(); // seeds the year
        let counts = s.count_colors().await.unwrap();
        assert!(counts.contains_key("9") && counts.contains_key("5"));
        assert!(!counts.contains_key("6"));

        // A custom type on color 6 with one event, then swap trip/gathering and delete the custom type.
        let n = |t: &str| Names {
            singular: t.into(),
            plural: t.into(),
        };
        let custom = TypeDef {
            kind: EventType::Custom(1),
            color_id: Some("6".into()),
            labels: Labels {
                fr: n("Sport"),
                en: n("Sport"),
            },
        };
        registry.set(TypeCatalog::with_overrides(vec![custom]).unwrap());
        let mut new = new_event("trip", "Judo", "2026-04-01", "2026-04-01");
        new.kind = EventType::Custom(1);
        s.create(&new, "Alex").await.unwrap();

        let mut trip = registry.current().get(EventType::Trip).unwrap().clone();
        let mut gathering = registry
            .current()
            .get(EventType::FamilyGathering)
            .unwrap()
            .clone();
        (trip.color_id, gathering.color_id) = (gathering.color_id.clone(), trip.color_id.clone());
        let mut defs = TypeCatalog::with_overrides(vec![trip, gathering])
            .unwrap()
            .defs()
            .to_vec();
        defs.retain(|d| d.kind != EventType::Custom(1));
        let target = TypeCatalog::new(defs).unwrap();
        let mapping = registry.current().color_mapping(&target);
        let before = s.list_year(2026).await.unwrap();
        let trips = before.iter().filter(|e| e.kind == EventType::Trip).count();
        let gatherings = before
            .iter()
            .filter(|e| e.kind == EventType::FamilyGathering)
            .count();
        s.recolor("m", &mapping, &target).await.unwrap();
        registry.set(target);
        let after = s.list_year(2026).await.unwrap();
        // Event types never change in a swap: only their colors (held by the catalog) do.
        assert_eq!(
            after.iter().filter(|e| e.kind == EventType::Trip).count(),
            trips
        );
        assert_eq!(
            after
                .iter()
                .filter(|e| e.kind == EventType::FamilyGathering)
                .count(),
            gatherings
        );
        let judo = after.iter().find(|e| e.title == "Judo").unwrap();
        assert_eq!(judo.kind, EventType::Unclassified);
    }
}
