//! Google event ⇄ domain event. The single place where exclusive/inclusive ends are converted.

use annee_domain::birthday::{age_at, recurrence_rule};
use annee_domain::dates::{exclusive_from_inclusive, inclusive_from_exclusive, timed_span};
use annee_domain::{CalendarKind, Event, EventId, EventType, NewEvent, Source, TypeCatalog};
use chrono::{DateTime, Datelike, NaiveDate};
use chrono_tz::Tz;
use serde_json::{Map, Value, json};

use super::model::{GEvent, GExtended, GTime};
use crate::recurrence::instance_id;

pub const SCHEMA: &str = "1";
pub const KEY_SCHEMA: &str = "fy_schema";
pub const KEY_MEMBERS: &str = "fy_members";
pub const KEY_BIRTH_YEAR: &str = "fy_birth_year";
pub const KEY_SOURCE: &str = "fy_source";
pub const KEY_SOURCE_ID: &str = "fy_source_id";
pub const KEY_CREATED_BY: &str = "fy_created_by";
pub const KEY_UPDATED_BY: &str = "fy_updated_by";
/// Id of the color migration that last rewrote the event (§19.5).
pub const KEY_MIGRATION: &str = "fy_migration";

pub struct MapCtx<'a> {
    pub tz: Tz,
    pub members: &'a [String],
    pub catalog: &'a TypeCatalog,
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Map a Google event to the domain model. `None` for cancelled events or events without a
/// usable start/end.
pub fn to_event(g: &GEvent, calendar: CalendarKind, ctx: &MapCtx) -> Option<Event> {
    if g.status.as_deref() == Some("cancelled") {
        return None;
    }
    let id = g.id.as_deref()?;
    let (gs, ge) = (g.start.as_ref()?, g.end.as_ref()?);
    let (start, end, start_time, end_time) = if let (Some(s), Some(e)) = (&gs.date, &ge.date) {
        let (s, e) = (parse_date(s)?, parse_date(e)?);
        // Google's all-day end is exclusive.
        (s, inclusive_from_exclusive(e).max(s), None, None)
    } else if let (Some(s), Some(e)) = (&gs.date_time, &ge.date_time) {
        let span = timed_span(
            DateTime::parse_from_rfc3339(s).ok()?,
            DateTime::parse_from_rfc3339(e).ok()?,
            ctx.tz,
        );
        (
            span.start,
            span.end,
            Some(span.start_time),
            Some(span.end_time),
        )
    } else {
        return None;
    };

    let kind = ctx.catalog.from_color_id(g.color_id.as_deref());
    let source = g.private(KEY_SOURCE).map(Source::parse).unwrap_or_default();
    let mut members = Vec::new();
    if let Some(csv) = g.private(KEY_MEMBERS) {
        for name in csv.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            if ctx.members.iter().any(|m| m == name) {
                if !members.iter().any(|m: &String| m == name) {
                    members.push(name.to_string());
                }
            } else {
                tracing::debug!(member = name, "ignoring unknown member in fy_members");
            }
        }
        // Keep the configured order.
        members.sort_by_key(|m| ctx.members.iter().position(|c| c == m));
    }
    let birth_year = g
        .private(KEY_BIRTH_YEAR)
        .and_then(|v| v.parse::<i32>().ok());
    let age = if kind == EventType::Birthday {
        age_at(birth_year, start.year())
    } else {
        None
    };
    let title = g
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("(Sans titre)");

    Some(Event {
        id: EventId::new(calendar, id),
        series_id: g
            .recurring_event_id
            .as_deref()
            .map(|r| EventId::new(calendar, r)),
        calendar,
        kind,
        title: title.to_string(),
        start,
        end,
        start_time,
        end_time,
        members,
        location: g.location.clone().filter(|s| !s.is_empty()),
        notes: g.description.clone().filter(|s| !s.is_empty()),
        birth_year,
        age,
        source,
        read_only: source.is_import(),
        created_by: g.private(KEY_CREATED_BY).map(String::from),
        updated_by: g.private(KEY_UPDATED_BY).map(String::from),
        google_url: g.html_link.clone(),
        source_id: g.private(KEY_SOURCE_ID).map(String::from),
    })
}

/// Turn the master event of a yearly series into the occurrence on `date`.
pub fn occurrence_of(master: &Event, date: NaiveDate) -> Event {
    let mut e = master.clone();
    let span = master.end - master.start;
    e.series_id = Some(master.id.clone());
    e.id = EventId::new(master.calendar, instance_id(&master.id.google_id, date));
    e.start = date;
    e.end = date + span;
    e.age = if e.kind == EventType::Birthday {
        age_at(e.birth_year, date.year())
    } else {
        None
    };
    e
}

fn members_csv(members: &[String]) -> Option<String> {
    (!members.is_empty()).then(|| members.join(","))
}

/// Body of `events.insert` for a user-created event.
pub fn insert_body(new: &NewEvent, writer: &str, tz: Tz, catalog: &TypeCatalog) -> GEvent {
    let (start, end, recurrence) = if new.kind == EventType::Birthday {
        let first = annee_domain::birthday::first_occurrence(new.start, new.birth_year)
            .unwrap_or(new.start);
        (first, first, Some(vec![recurrence_rule(first)]))
    } else {
        (new.start, new.end, None)
    };
    let mut private = std::collections::BTreeMap::new();
    private.insert(KEY_SCHEMA.to_string(), SCHEMA.to_string());
    if let Some(m) = members_csv(&new.members) {
        private.insert(KEY_MEMBERS.to_string(), m);
    }
    if let Some(y) = new.birth_year {
        private.insert(KEY_BIRTH_YEAR.to_string(), y.to_string());
    }
    private.insert(KEY_SOURCE.to_string(), Source::Manual.as_str().to_string());
    private.insert(KEY_CREATED_BY.to_string(), writer.to_string());
    private.insert(KEY_UPDATED_BY.to_string(), writer.to_string());
    let _ = tz;
    GEvent {
        summary: Some(new.title.clone()),
        description: new.notes.clone(),
        location: new.location.clone(),
        color_id: catalog.color_id(new.kind).map(String::from),
        start: Some(GTime::all_day(start.to_string())),
        end: Some(GTime::all_day(exclusive_from_inclusive(end).to_string())),
        recurrence,
        extended_properties: Some(GExtended { private }),
        ..Default::default()
    }
}

/// Body of `events.insert` for an imported event.
pub fn insert_imported_body(e: &crate::store::ImportedEvent, catalog: &TypeCatalog) -> GEvent {
    let mut private = std::collections::BTreeMap::new();
    private.insert(KEY_SCHEMA.to_string(), SCHEMA.to_string());
    private.insert(KEY_SOURCE.to_string(), e.source.as_str().to_string());
    private.insert(KEY_SOURCE_ID.to_string(), e.source_id.clone());
    GEvent {
        summary: Some(e.title.clone()),
        color_id: catalog.color_id(e.kind).map(String::from),
        start: Some(GTime::all_day(e.start.to_string())),
        end: Some(GTime::all_day(exclusive_from_inclusive(e.end).to_string())),
        extended_properties: Some(GExtended { private }),
        ..Default::default()
    }
}

/// Patch for an imported event whose title or dates changed.
pub fn imported_patch(e: &crate::store::ImportedEvent) -> Value {
    json!({
        "summary": e.title,
        "start": {"date": e.start.to_string()},
        "end": {"date": exclusive_from_inclusive(e.end).to_string()},
    })
}

/// Body of the `events.patch` that rewrites the color of an event during a migration.
pub fn recolor_patch(migration_id: &str, color: Option<&str>) -> Value {
    json!({
        "colorId": color,
        "extendedProperties": {"private": {KEY_MIGRATION: migration_id}},
    })
}

/// How the dates of a series (or single event) must be written.
pub struct DatePatch {
    pub start: NaiveDate,
    pub end_inclusive: NaiveDate,
    pub recurrence: Option<Vec<String>>,
}

/// Build the `events.patch` body: only what changed between `current` (the event as the user
/// sees it) and `new`. `dates` is the already-computed write for the start/end/recurrence.
pub fn patch_body(
    current: &Event,
    new: &NewEvent,
    writer: &str,
    dates: Option<DatePatch>,
    color: Option<Option<&str>>,
) -> Value {
    let mut body = Map::new();
    if new.title != current.title {
        body.insert("summary".into(), json!(new.title));
    }
    if new.location != current.location {
        body.insert("location".into(), json!(new.location));
    }
    if new.notes != current.notes {
        body.insert("description".into(), json!(new.notes));
    }
    if let Some(color) = color {
        body.insert("colorId".into(), json!(color));
    }
    if let Some(d) = dates {
        body.insert(
            "start".into(),
            json!({"date": d.start.to_string(), "dateTime": null, "timeZone": null}),
        );
        body.insert(
            "end".into(),
            json!({"date": exclusive_from_inclusive(d.end_inclusive).to_string(), "dateTime": null, "timeZone": null}),
        );
        if let Some(r) = d.recurrence {
            body.insert("recurrence".into(), json!(r));
        }
    }
    let mut private = Map::new();
    private.insert(KEY_SCHEMA.into(), json!(SCHEMA));
    private.insert(KEY_MEMBERS.into(), json!(members_csv(&new.members)));
    private.insert(
        KEY_BIRTH_YEAR.into(),
        json!(new.birth_year.map(|y| y.to_string())),
    );
    private.insert(KEY_UPDATED_BY.into(), json!(writer));
    body.insert("extendedProperties".into(), json!({"private": private}));
    Value::Object(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use annee_domain::dates::ymd;
    use chrono_tz::Europe::Paris;

    fn members() -> Vec<String> {
        ["Alex", "Camille", "Sacha", "Noé"]
            .map(String::from)
            .to_vec()
    }

    fn map(json: &str) -> Option<Event> {
        let g: GEvent = serde_json::from_str(json).unwrap();
        let m = members();
        to_event(
            &g,
            CalendarKind::Famille,
            &MapCtx {
                tz: Paris,
                members: &m,
                catalog: &TypeCatalog::default(),
            },
        )
    }

    #[test]
    fn all_day_event_has_inclusive_end() {
        let e = map(r#"{"id":"a1","summary":"Ski","colorId":"9",
            "start":{"date":"2026-02-21"},"end":{"date":"2026-03-01"},
            "extendedProperties":{"private":{"fy_members":"Sacha,Alex,Papi","fy_source":"manual","fy_created_by":"Alex"}},
            "htmlLink":"https://example/x"}"#)
            .unwrap();
        assert_eq!(e.kind, EventType::Trip);
        assert_eq!((e.start, e.end), (ymd(2026, 2, 21), ymd(2026, 2, 28)));
        assert_eq!(
            e.members,
            vec!["Alex", "Sacha"],
            "unknown dropped, configured order"
        );
        assert_eq!(e.created_by.as_deref(), Some("Alex"));
        assert_eq!(e.google_url.as_deref(), Some("https://example/x"));
        assert!(!e.read_only);
        assert_eq!(e.id.to_string(), "f:a1");
    }

    #[test]
    fn single_day_event() {
        let e = map(r#"{"id":"a1","summary":"x","start":{"date":"2026-03-14"},"end":{"date":"2026-03-15"}}"#).unwrap();
        assert_eq!((e.start, e.end), (ymd(2026, 3, 14), ymd(2026, 3, 14)));
        assert_eq!(e.kind, EventType::Unclassified);
    }

    #[test]
    fn timed_event_keeps_times_and_local_dates() {
        let e = map(r#"{"id":"a1","summary":"Fête","colorId":"3",
            "start":{"dateTime":"2026-03-14T15:00:00+01:00"},"end":{"dateTime":"2026-03-14T23:00:00+01:00"}}"#)
            .unwrap();
        assert_eq!((e.start, e.end), (ymd(2026, 3, 14), ymd(2026, 3, 14)));
        assert_eq!(e.start_time.unwrap().to_string(), "15:00:00");
        assert_eq!(e.end_time.unwrap().to_string(), "23:00:00");
    }

    #[test]
    fn cancelled_and_malformed_events_are_dropped() {
        assert!(map(r#"{"id":"a","status":"cancelled","start":{"date":"2026-03-14"},"end":{"date":"2026-03-15"}}"#).is_none());
        assert!(map(r#"{"id":"a","summary":"x"}"#).is_none());
    }

    #[test]
    fn imported_events_are_read_only() {
        let e = map(r#"{"id":"a","summary":"Toussaint","colorId":"5",
            "start":{"date":"2025-10-18"},"end":{"date":"2025-11-03"},
            "extendedProperties":{"private":{"fy_source":"import:education","fy_source_id":"edu:2025-2026:toussaint:2025-10-18"}}}"#)
            .unwrap();
        assert!(e.read_only);
        assert_eq!(e.source, Source::ImportEducation);
        assert_eq!(
            e.source_id.as_deref(),
            Some("edu:2025-2026:toussaint:2025-10-18")
        );
        assert_eq!(e.end, ymd(2025, 11, 2));
    }

    #[test]
    fn birthday_instance_has_age_and_series() {
        let g: GEvent = serde_json::from_str(
            r#"{"id":"b_20260312","summary":"Anniversaire de Sacha","colorId":"2","recurringEventId":"b",
            "start":{"date":"2026-03-12"},"end":{"date":"2026-03-13"},
            "extendedProperties":{"private":{"fy_birth_year":"2018"}}}"#,
        )
        .unwrap();
        let m = members();
        let e = to_event(
            &g,
            CalendarKind::Reperes,
            &MapCtx {
                tz: Paris,
                members: &m,
                catalog: &TypeCatalog::default(),
            },
        )
        .unwrap();
        assert_eq!(e.age, Some(8));
        assert_eq!(e.series_id.unwrap().to_string(), "r:b");
        assert_eq!(e.birth_year, Some(2018));
    }

    #[test]
    fn untitled_event_gets_a_label() {
        let e =
            map(r#"{"id":"a","start":{"date":"2026-03-14"},"end":{"date":"2026-03-15"}}"#).unwrap();
        assert_eq!(e.title, "(Sans titre)");
    }

    #[test]
    fn insert_body_for_a_trip() {
        let new = NewEvent {
            kind: EventType::Trip,
            title: "Lisbonne".into(),
            start: ymd(2026, 5, 14),
            end: ymd(2026, 5, 17),
            start_time: None,
            end_time: None,
            members: vec!["Alex".into(), "Camille".into()],
            location: Some("Portugal".into()),
            notes: None,
            birth_year: None,
        };
        let v = serde_json::to_value(insert_body(&new, "Alex", Paris, &TypeCatalog::default()))
            .unwrap();
        assert_eq!(v["colorId"], "9");
        assert_eq!(v["start"], json!({"date": "2026-05-14"}));
        assert_eq!(v["end"], json!({"date": "2026-05-18"}), "exclusive end");
        let p = &v["extendedProperties"]["private"];
        assert_eq!(p["fy_schema"], "1");
        assert_eq!(p["fy_members"], "Alex,Camille");
        assert_eq!(p["fy_source"], "manual");
        assert_eq!(p["fy_created_by"], "Alex");
        assert!(p.get("fy_birth_year").is_none(), "no empty values");
        assert!(v.get("description").is_none());
        assert!(v.get("recurrence").is_none());
    }

    #[test]
    fn insert_body_for_a_birthday_uses_birth_date_and_rrule() {
        let new = NewEvent {
            kind: EventType::Birthday,
            title: "Anniversaire de Sacha".into(),
            start: ymd(2026, 3, 12),
            end: ymd(2026, 3, 12),
            start_time: None,
            end_time: None,
            members: vec![],
            location: None,
            notes: None,
            birth_year: Some(2018),
        };
        let v = serde_json::to_value(insert_body(&new, "Alex", Paris, &TypeCatalog::default()))
            .unwrap();
        assert_eq!(v["colorId"], "2");
        assert_eq!(v["start"]["date"], "2018-03-12");
        assert_eq!(v["end"]["date"], "2018-03-13");
        assert_eq!(v["recurrence"], json!(["RRULE:FREQ=YEARLY"]));
        assert_eq!(v["extendedProperties"]["private"]["fy_birth_year"], "2018");
        assert!(
            v["extendedProperties"]["private"]
                .get("fy_members")
                .is_none()
        );
    }

    #[test]
    fn leap_day_birthday_rrule() {
        let new = NewEvent {
            kind: EventType::Birthday,
            title: "B".into(),
            start: ymd(2028, 2, 29),
            end: ymd(2028, 2, 29),
            start_time: None,
            end_time: None,
            members: vec![],
            location: None,
            notes: None,
            birth_year: Some(2020),
        };
        let v =
            serde_json::to_value(insert_body(&new, "T", Paris, &TypeCatalog::default())).unwrap();
        assert_eq!(
            v["recurrence"],
            json!(["RRULE:FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=-1"])
        );
        assert_eq!(v["start"]["date"], "2020-02-29");
    }
}
