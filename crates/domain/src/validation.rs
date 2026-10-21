//! Input validation for create and update (§5.2). All messages are French.

use chrono::{Datelike, NaiveDate, NaiveTime};
use serde::{Deserialize, Deserializer};

use crate::birthday::birth_date_possible;
use crate::catalog::TypeCatalog;
use crate::lang::Lang;
use crate::types::{Event, EventType};

pub const MAX_TITLE: usize = 120;
pub const MAX_LOCATION: usize = 200;
pub const MAX_NOTES: usize = 2000;
pub const MAX_SPAN_DAYS: i64 = 366;
/// Client-side only message (the web form requires at least one checked person).
pub const AT_LEAST_ONE_PERSON: &str = "Choisissez au moins une personne.";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("Donnez un titre à l’événement.")]
    TitleRequired,
    #[error("Le titre est trop long (120 caractères maximum).")]
    TitleTooLong,
    #[error("Choisissez un premier et un dernier jour.")]
    InvalidDates,
    #[error("Le dernier jour ne peut pas être avant le premier.")]
    EndBeforeStart,
    #[error("Un événement ne peut pas dépasser un an.")]
    SpanTooLong,
    #[error("Ce type d’événement ne peut pas être créé à la main.")]
    TypeNotWritable,
    #[error("Un anniversaire dure un seul jour.")]
    BirthdaySingleDay,
    #[error("Personne inconnue : {0}.")]
    UnknownMember(String),
    #[error("Année de naissance invalide.")]
    InvalidBirthYear,
    #[error("Cet événement est importé automatiquement et ne peut pas être modifié.")]
    ReadOnly,
    #[error("Choisissez une catégorie.")]
    TypeRequired,
    #[error("Catégorie inconnue : {0}.")]
    UnknownType(String),
    #[error("Le lieu est trop long (200 caractères maximum).")]
    LocationTooLong,
    #[error("Les notes sont trop longues (2000 caractères maximum).")]
    NotesTooLong,
}

impl ValidationError {
    /// The message in `lang` (the `Display` text is the French one).
    pub fn message(&self, lang: Lang) -> String {
        if lang == Lang::Fr {
            return self.to_string();
        }
        match self {
            ValidationError::TitleRequired => "Give the event a title.".into(),
            ValidationError::TitleTooLong => {
                "The title is too long (120 characters maximum).".into()
            }
            ValidationError::InvalidDates => "Choose a first and a last day.".into(),
            ValidationError::EndBeforeStart => {
                "The last day cannot be before the first day.".into()
            }
            ValidationError::SpanTooLong => "An event cannot last more than a year.".into(),
            ValidationError::TypeNotWritable => {
                "This type of event cannot be created by hand.".into()
            }
            ValidationError::BirthdaySingleDay => "A birthday lasts a single day.".into(),
            ValidationError::UnknownMember(n) => format!("Unknown person: {n}."),
            ValidationError::InvalidBirthYear => "Invalid birth year.".into(),
            ValidationError::ReadOnly => {
                "This event is imported automatically and cannot be modified.".into()
            }
            ValidationError::TypeRequired => "Choose a category.".into(),
            ValidationError::UnknownType(c) => format!("Unknown category: {c}."),
            ValidationError::LocationTooLong => {
                "The place is too long (200 characters maximum).".into()
            }
            ValidationError::NotesTooLong => {
                "The notes are too long (2000 characters maximum).".into()
            }
        }
    }

    /// Stable English code for API and MCP errors.
    pub fn code(&self) -> &'static str {
        match self {
            ValidationError::ReadOnly => "read_only",
            _ => "validation_failed",
        }
    }
}

/// Facts the validation needs from the configuration and the clock.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    pub members: &'a [String],
    pub today: NaiveDate,
    pub catalog: &'a TypeCatalog,
}

/// Raw input of `POST /api/events` and the MCP `create_event` tool.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EventInput {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub members: Option<Vec<String>>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub birth_year: Option<i32>,
}

fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Raw input of `PATCH /api/events/{id}` and the MCP `update_event` tool. Omitted fields are left
/// unchanged; `null` clears `location`, `notes` and `birth_year`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EventPatch {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub members: Option<Vec<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub location: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub birth_year: Option<Option<i32>>,
}

/// A fully validated, normalized event state, ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    pub kind: EventType,
    pub title: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Kept only when an update does not touch the dates of a timed event.
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    /// Empty = whole family.
    pub members: Vec<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub birth_year: Option<i32>,
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

fn clean_title(raw: &str) -> Result<String, ValidationError> {
    let title = raw.trim();
    if title.is_empty() {
        return Err(ValidationError::TitleRequired);
    }
    if title.chars().count() > MAX_TITLE {
        return Err(ValidationError::TitleTooLong);
    }
    Ok(title.to_string())
}

fn clean_optional(
    raw: Option<&str>,
    max: usize,
    err: ValidationError,
) -> Result<Option<String>, ValidationError> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if s.chars().count() > max {
        return Err(err);
    }
    Ok(Some(s.to_string()))
}

/// Members must be configured names. The result follows the configured order, without
/// duplicates; selecting everybody is stored as an empty list ("whole family").
pub fn normalize_members(
    members: &[String],
    configured: &[String],
) -> Result<Vec<String>, ValidationError> {
    for m in members {
        if !configured.contains(m) {
            return Err(ValidationError::UnknownMember(m.clone()));
        }
    }
    let ordered: Vec<String> = configured
        .iter()
        .filter(|c| members.contains(c))
        .cloned()
        .collect();
    if ordered.len() == configured.len() {
        return Ok(Vec::new());
    }
    Ok(ordered)
}

fn check_birth_year(
    kind: EventType,
    birth_year: Option<i32>,
    start: NaiveDate,
    ctx: &ValidationContext,
) -> Result<Option<i32>, ValidationError> {
    if kind != EventType::Birthday {
        return Ok(None);
    }
    let Some(y) = birth_year else { return Ok(None) };
    if !(1900..=ctx.today.year()).contains(&y)
        || !birth_date_possible(start.month(), start.day(), y)
    {
        return Err(ValidationError::InvalidBirthYear);
    }
    Ok(Some(y))
}

/// Shared checks, in the order of the spec's table.
#[allow(clippy::too_many_arguments)]
fn finish(
    kind: EventType,
    title: &str,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    members: &[String],
    location: Option<&str>,
    notes: Option<&str>,
    birth_year: Option<i32>,
    check_writable: bool,
    ctx: &ValidationContext,
) -> Result<NewEvent, ValidationError> {
    let title = clean_title(title)?;
    let (Some(start), Some(end)) = (start, end) else {
        return Err(ValidationError::InvalidDates);
    };
    if end < start {
        return Err(ValidationError::EndBeforeStart);
    }
    if (end - start).num_days() + 1 > MAX_SPAN_DAYS {
        return Err(ValidationError::SpanTooLong);
    }
    if check_writable && !kind.writable() {
        return Err(ValidationError::TypeNotWritable);
    }
    if kind == EventType::Birthday && start != end {
        return Err(ValidationError::BirthdaySingleDay);
    }
    let members = normalize_members(members, ctx.members)?;
    let birth_year = check_birth_year(kind, birth_year, start, ctx)?;
    let location = clean_optional(location, MAX_LOCATION, ValidationError::LocationTooLong)?;
    let notes = clean_optional(notes, MAX_NOTES, ValidationError::NotesTooLong)?;
    Ok(NewEvent {
        kind,
        title,
        start,
        end,
        start_time: None,
        end_time: None,
        members,
        location,
        notes,
        birth_year,
    })
}

fn parse_kind(code: &str, catalog: &TypeCatalog) -> Result<EventType, ValidationError> {
    EventType::from_code(code.trim())
        .filter(|k| catalog.contains(*k))
        .ok_or_else(|| ValidationError::UnknownType(code.trim().to_string()))
}

/// Validate a creation request.
pub fn validate_new(
    input: &EventInput,
    ctx: &ValidationContext,
) -> Result<NewEvent, ValidationError> {
    let kind = parse_kind(
        input.kind.as_deref().ok_or(ValidationError::TypeRequired)?,
        ctx.catalog,
    )?;
    let start = input.start.as_deref().and_then(parse_date);
    // A missing end defaults to the start (single-day event); a present but invalid one is an error.
    let end = match input.end.as_deref() {
        None => start,
        Some(s) => parse_date(s),
    };
    // Title is checked first by the table, but a missing type must not mask it.
    finish(
        kind,
        input.title.as_deref().unwrap_or(""),
        start,
        end,
        input.members.as_deref().unwrap_or(&[]),
        input.location.as_deref(),
        input.notes.as_deref(),
        input.birth_year,
        true,
        ctx,
    )
}

/// Apply a patch to an existing event and validate the result.
pub fn validate_update(
    existing: &Event,
    patch: &EventPatch,
    ctx: &ValidationContext,
) -> Result<NewEvent, ValidationError> {
    if existing.read_only {
        return Err(ValidationError::ReadOnly);
    }
    let (kind, kind_given) = match patch.kind.as_deref() {
        Some(code) => (parse_kind(code, ctx.catalog)?, true),
        None => (existing.kind, false),
    };
    let title = patch.title.as_deref().unwrap_or(&existing.title);
    let start = match patch.start.as_deref() {
        Some(s) => parse_date(s),
        None => Some(existing.start),
    };
    let end = match patch.end.as_deref() {
        Some(s) => parse_date(s),
        // A birthday lasts one day: its end follows its start.
        None if kind == EventType::Birthday && patch.start.is_some() => start,
        None => Some(existing.end),
    };
    let members = patch.members.as_deref().unwrap_or(&existing.members);
    let location = match &patch.location {
        Some(v) => v.as_deref(),
        None => existing.location.as_deref(),
    };
    let notes = match &patch.notes {
        Some(v) => v.as_deref(),
        None => existing.notes.as_deref(),
    };
    let birth_year = match patch.birth_year {
        Some(v) => v,
        None => existing.birth_year,
    };
    let mut new = finish(
        kind, title, start, end, members, location, notes, birth_year, kind_given, ctx,
    )?;
    // Timed events stay timed unless their dates change.
    if existing.start_time.is_some() && new.start == existing.start && new.end == existing.end {
        new.start_time = existing.start_time;
        new.end_time = existing.end_time;
    }
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::ymd;
    use crate::types::{CalendarKind, EventId, Source};

    fn members() -> Vec<String> {
        ["Alex", "Camille", "Sacha", "Noé"]
            .map(String::from)
            .to_vec()
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

    fn input() -> EventInput {
        EventInput {
            kind: Some("trip".into()),
            title: Some("Road trip".into()),
            start: Some("2026-07-25".into()),
            end: Some("2026-08-02".into()),
            ..Default::default()
        }
    }

    fn err(i: EventInput) -> String {
        let m = members();
        validate_new(&i, &ctx(&m)).unwrap_err().to_string()
    }

    fn existing() -> Event {
        Event {
            id: EventId::new(CalendarKind::Famille, "abc"),
            series_id: None,
            calendar: CalendarKind::Famille,
            kind: EventType::Trip,
            title: "Road trip".into(),
            start: ymd(2026, 7, 25),
            end: ymd(2026, 8, 2),
            start_time: None,
            end_time: None,
            members: vec![],
            location: None,
            notes: None,
            birth_year: None,
            age: None,
            source: Source::Manual,
            read_only: false,
            created_by: None,
            updated_by: None,
            google_url: None,
            source_id: None,
        }
    }

    #[test]
    fn valid_input_passes_and_is_normalized() {
        let m = members();
        let mut i = input();
        i.title = Some("  Road trip  ".into());
        i.location = Some("   ".into());
        let n = validate_new(&i, &ctx(&m)).unwrap();
        assert_eq!(n.title, "Road trip");
        assert_eq!(n.location, None);
        assert!(n.members.is_empty());
    }

    #[test]
    fn title_rules() {
        let mut i = input();
        i.title = Some("   ".into());
        assert_eq!(err(i), "Donnez un titre à l’événement.");
        let mut i = input();
        i.title = None;
        assert_eq!(err(i), "Donnez un titre à l’événement.");
        let mut i = input();
        i.title = Some("é".repeat(121));
        assert_eq!(err(i), "Le titre est trop long (120 caractères maximum).");
        let mut i = input();
        i.title = Some("é".repeat(120));
        assert!(validate_new(&i, &ctx(&members())).is_ok());
    }

    #[test]
    fn date_rules() {
        let mut i = input();
        i.start = None;
        assert_eq!(err(i), "Choisissez un premier et un dernier jour.");
        let mut i = input();
        i.end = Some("pas une date".into());
        assert_eq!(err(i), "Choisissez un premier et un dernier jour.");
        let mut i = input();
        i.start = Some("2026-02-30".into());
        assert_eq!(err(i), "Choisissez un premier et un dernier jour.");
        let mut i = input();
        i.end = Some("2026-07-24".into());
        assert_eq!(err(i), "Le dernier jour ne peut pas être avant le premier.");
        let mut i = input();
        i.start = Some("2026-01-01".into());
        i.end = Some("2027-01-02".into());
        assert_eq!(err(i), "Un événement ne peut pas dépasser un an.");
    }

    #[test]
    fn span_limit_is_366_days_inclusive() {
        let m = members();
        let mut i = input();
        i.start = Some("2028-01-01".into());
        i.end = Some("2028-12-31".into()); // 366 days
        assert!(validate_new(&i, &ctx(&m)).is_ok());
        i.end = Some("2029-01-01".into()); // 367 days
        assert!(validate_new(&i, &ctx(&m)).is_err());
    }

    #[test]
    fn missing_end_defaults_to_start() {
        let m = members();
        let mut i = input();
        i.end = None;
        let n = validate_new(&i, &ctx(&m)).unwrap();
        assert_eq!(n.start, n.end);
    }

    #[test]
    fn type_rules() {
        for code in ["school_holiday", "public_holiday", "unclassified"] {
            let mut i = input();
            i.kind = Some(code.into());
            assert_eq!(
                err(i),
                "Ce type d’événement ne peut pas être créé à la main."
            );
        }
        let mut i = input();
        i.kind = None;
        assert_eq!(err(i), "Choisissez une catégorie.");
        let mut i = input();
        i.kind = Some("party".into());
        assert_eq!(err(i), "Catégorie inconnue : party.");
    }

    #[test]
    fn birthday_is_one_day() {
        let mut i = input();
        i.kind = Some("birthday".into());
        assert_eq!(err(i), "Un anniversaire dure un seul jour.");
        let m = members();
        let mut i = input();
        i.kind = Some("birthday".into());
        i.end = Some("2026-07-25".into());
        assert!(validate_new(&i, &ctx(&m)).is_ok());
    }

    #[test]
    fn member_rules() {
        let mut i = input();
        i.members = Some(vec!["Alex".into(), "Papi".into()]);
        assert_eq!(err(i), "Personne inconnue : Papi.");
        let m = members();
        let mut i = input();
        i.members = Some(vec!["Noé".into(), "Alex".into(), "Noé".into()]);
        assert_eq!(
            validate_new(&i, &ctx(&m)).unwrap().members,
            vec!["Alex", "Noé"]
        );
        i.members = Some(m.iter().rev().cloned().collect());
        assert!(
            validate_new(&i, &ctx(&m)).unwrap().members.is_empty(),
            "everybody = whole family"
        );
        i.members = Some(vec![]);
        assert!(validate_new(&i, &ctx(&m)).unwrap().members.is_empty());
    }

    #[test]
    fn birth_year_rules() {
        let m = members();
        let mut i = input();
        i.kind = Some("birthday".into());
        i.end = Some("2026-07-25".into());
        i.birth_year = Some(2018);
        assert_eq!(validate_new(&i, &ctx(&m)).unwrap().birth_year, Some(2018));
        for y in [1899, 2027, -5] {
            i.birth_year = Some(y);
            assert_eq!(err(i.clone()), "Année de naissance invalide.");
        }
        i.birth_year = Some(1900);
        assert!(validate_new(&i, &ctx(&m)).is_ok());
        i.birth_year = Some(2026);
        assert!(validate_new(&i, &ctx(&m)).is_ok());
        // 29 February needs a leap birth year.
        i.start = Some("2028-02-29".into());
        i.end = Some("2028-02-29".into());
        i.birth_year = Some(2019);
        assert_eq!(err(i.clone()), "Année de naissance invalide.");
        i.birth_year = Some(2020);
        assert!(validate_new(&i, &ctx(&m)).is_ok());
    }

    #[test]
    fn birth_year_is_dropped_for_other_types() {
        let m = members();
        let mut i = input();
        i.birth_year = Some(2018);
        assert_eq!(validate_new(&i, &ctx(&m)).unwrap().birth_year, None);
    }

    #[test]
    fn length_limits_on_location_and_notes() {
        let mut i = input();
        i.location = Some("x".repeat(201));
        assert_eq!(err(i), "Le lieu est trop long (200 caractères maximum).");
        let mut i = input();
        i.notes = Some("x".repeat(2001));
        assert_eq!(
            err(i),
            "Les notes sont trop longues (2000 caractères maximum)."
        );
    }

    #[test]
    fn read_only_events_cannot_be_updated() {
        let m = members();
        let mut e = existing();
        e.read_only = true;
        let e = validate_update(&e, &EventPatch::default(), &ctx(&m)).unwrap_err();
        assert_eq!(
            e.to_string(),
            "Cet événement est importé automatiquement et ne peut pas être modifié."
        );
        assert_eq!(e.code(), "read_only");
    }

    #[test]
    fn update_merges_and_validates() {
        let m = members();
        let patch = EventPatch {
            title: Some("Italie".into()),
            ..Default::default()
        };
        let n = validate_update(&existing(), &patch, &ctx(&m)).unwrap();
        assert_eq!(n.title, "Italie");
        assert_eq!(n.start, ymd(2026, 7, 25));
        let patch = EventPatch {
            end: Some("2026-07-01".into()),
            ..Default::default()
        };
        assert_eq!(
            validate_update(&existing(), &patch, &ctx(&m)).unwrap_err(),
            ValidationError::EndBeforeStart
        );
    }

    #[test]
    fn update_null_clears_optional_fields() {
        let m = members();
        let mut e = existing();
        e.location = Some("Rome".into());
        e.notes = Some("n".into());
        let patch: EventPatch = serde_json::from_str(r#"{"location": null}"#).unwrap();
        let n = validate_update(&e, &patch, &ctx(&m)).unwrap();
        assert_eq!(n.location, None);
        assert_eq!(
            n.notes.as_deref(),
            Some("n"),
            "omitted fields stay unchanged"
        );
        let patch: EventPatch = serde_json::from_str(r#"{"notes": "autre"}"#).unwrap();
        assert_eq!(
            validate_update(&e, &patch, &ctx(&m))
                .unwrap()
                .notes
                .as_deref(),
            Some("autre")
        );
    }

    #[test]
    fn update_to_unwritable_type_is_rejected_but_keeping_unclassified_is_allowed() {
        let m = members();
        let patch = EventPatch {
            kind: Some("school_holiday".into()),
            ..Default::default()
        };
        assert_eq!(
            validate_update(&existing(), &patch, &ctx(&m)).unwrap_err(),
            ValidationError::TypeNotWritable
        );
        let mut e = existing();
        e.kind = EventType::Unclassified;
        let patch = EventPatch {
            title: Some("Renommé".into()),
            ..Default::default()
        };
        assert_eq!(
            validate_update(&e, &patch, &ctx(&m)).unwrap().kind,
            EventType::Unclassified
        );
        let patch = EventPatch {
            kind: Some("celebration".into()),
            ..Default::default()
        };
        assert_eq!(
            validate_update(&e, &patch, &ctx(&m)).unwrap().kind,
            EventType::Celebration
        );
    }

    #[test]
    fn timed_event_stays_timed_unless_dates_change() {
        let m = members();
        let mut e = existing();
        e.start = ymd(2026, 3, 14);
        e.end = ymd(2026, 3, 14);
        e.start_time = NaiveTime::from_hms_opt(15, 0, 0);
        e.end_time = NaiveTime::from_hms_opt(23, 0, 0);
        let patch = EventPatch {
            title: Some("Fête".into()),
            ..Default::default()
        };
        let n = validate_update(&e, &patch, &ctx(&m)).unwrap();
        assert_eq!(n.start_time, e.start_time);
        assert_eq!(n.end_time, e.end_time);
        let patch = EventPatch {
            start: Some("2026-03-15".into()),
            end: Some("2026-03-15".into()),
            ..Default::default()
        };
        let n = validate_update(&e, &patch, &ctx(&m)).unwrap();
        assert_eq!(n.start_time, None);
        assert_eq!(n.end_time, None);
    }

    #[test]
    fn birthday_end_follows_start_on_update() {
        let m = members();
        let mut e = existing();
        e.kind = EventType::Birthday;
        e.start = ymd(2026, 3, 12);
        e.end = ymd(2026, 3, 12);
        let patch = EventPatch {
            start: Some("2026-03-13".into()),
            ..Default::default()
        };
        let n = validate_update(&e, &patch, &ctx(&m)).unwrap();
        assert_eq!((n.start, n.end), (ymd(2026, 3, 13), ymd(2026, 3, 13)));
    }
}
