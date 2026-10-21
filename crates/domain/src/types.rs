//! Core types: calendars, event types (§4.2), identifiers and the normalized `Event` (§5.1).

use std::fmt;
use std::str::FromStr;

use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The two Google calendars the app reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarKind {
    Reperes,
    Famille,
}

impl CalendarKind {
    /// Prefix used in event ids (`r:` / `f:`).
    pub fn prefix(self) -> &'static str {
        match self {
            CalendarKind::Reperes => "r",
            CalendarKind::Famille => "f",
        }
    }

    pub fn from_prefix(p: &str) -> Option<Self> {
        match p {
            "r" => Some(CalendarKind::Reperes),
            "f" => Some(CalendarKind::Famille),
            _ => None,
        }
    }
}

/// How a type is drawn on the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderKind {
    Tint,
    Bar,
}

/// Category of an event, derived only from the Google `colorId` (§4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    SchoolHoliday,
    DaycareClosure,
    Birthday,
    PublicHoliday,
    FamilyGathering,
    FamilyHome,
    Trip,
    Celebration,
    Unclassified,
}

impl EventType {
    pub const ALL: [EventType; 9] = [
        EventType::SchoolHoliday,
        EventType::DaycareClosure,
        EventType::Birthday,
        EventType::PublicHoliday,
        EventType::FamilyGathering,
        EventType::FamilyHome,
        EventType::Trip,
        EventType::Celebration,
        EventType::Unclassified,
    ];

    /// The six types users can create and edit, in the order of the dialog's select.
    pub const WRITABLE: [EventType; 6] = [
        EventType::Birthday,
        EventType::Celebration,
        EventType::FamilyGathering,
        EventType::FamilyHome,
        EventType::Trip,
        EventType::DaycareClosure,
    ];

    /// Machine code (also the JSON value).
    pub fn code(self) -> &'static str {
        match self {
            EventType::SchoolHoliday => "school_holiday",
            EventType::DaycareClosure => "daycare_closure",
            EventType::Birthday => "birthday",
            EventType::PublicHoliday => "public_holiday",
            EventType::FamilyGathering => "family_gathering",
            EventType::FamilyHome => "family_home",
            EventType::Trip => "trip",
            EventType::Celebration => "celebration",
            EventType::Unclassified => "unclassified",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        EventType::ALL.into_iter().find(|t| t.code() == code)
    }

    /// Type for a Google `colorId`. Unknown or missing ids are `Unclassified`.
    pub fn from_color_id(color_id: Option<&str>) -> Self {
        match color_id {
            Some("5") => EventType::SchoolHoliday,
            Some("4") => EventType::DaycareClosure,
            Some("2") => EventType::Birthday,
            Some("8") => EventType::PublicHoliday,
            Some("11") => EventType::FamilyGathering,
            Some("10") => EventType::FamilyHome,
            Some("9") => EventType::Trip,
            Some("3") => EventType::Celebration,
            _ => EventType::Unclassified,
        }
    }

    /// Google `colorId` written for this type (`None` for `Unclassified`).
    pub fn color_id(self) -> Option<&'static str> {
        match self {
            EventType::SchoolHoliday => Some("5"),
            EventType::DaycareClosure => Some("4"),
            EventType::Birthday => Some("2"),
            EventType::PublicHoliday => Some("8"),
            EventType::FamilyGathering => Some("11"),
            EventType::FamilyHome => Some("10"),
            EventType::Trip => Some("9"),
            EventType::Celebration => Some("3"),
            EventType::Unclassified => None,
        }
    }

    /// Calendar where new events of this type are written (`None` for `Unclassified`).
    pub fn home_calendar(self) -> Option<CalendarKind> {
        match self {
            EventType::SchoolHoliday
            | EventType::DaycareClosure
            | EventType::Birthday
            | EventType::PublicHoliday => Some(CalendarKind::Reperes),
            EventType::FamilyGathering
            | EventType::FamilyHome
            | EventType::Trip
            | EventType::Celebration => Some(CalendarKind::Famille),
            EventType::Unclassified => None,
        }
    }

    /// French singular label.
    pub fn label(self) -> &'static str {
        match self {
            EventType::SchoolHoliday => "Vacances scolaires",
            EventType::DaycareClosure => "Fermeture de la crèche ou du centre de loisirs",
            EventType::Birthday => "Anniversaire",
            EventType::PublicHoliday => "Jour férié",
            EventType::FamilyGathering => "Réunion de famille",
            EventType::FamilyHome => "Séjour en maison de famille",
            EventType::Trip => "Voyage ou vacances",
            EventType::Celebration => "Fête ou cérémonie",
            EventType::Unclassified => "Non classé",
        }
    }

    /// French plural label used in the legend.
    pub fn label_plural(self) -> &'static str {
        match self {
            EventType::SchoolHoliday => "Vacances scolaires",
            EventType::DaycareClosure => "Crèche / loisirs fermés",
            EventType::Birthday => "Anniversaires",
            EventType::PublicHoliday => "Jours fériés",
            EventType::FamilyGathering => "Réunions de famille",
            EventType::FamilyHome => "Maisons de famille",
            EventType::Trip => "Voyages & vacances",
            EventType::Celebration => "Fêtes & cérémonies",
            EventType::Unclassified => "Non classés",
        }
    }

    /// Users can create events of this type (and imported types cannot be edited).
    pub fn writable(self) -> bool {
        EventType::WRITABLE.contains(&self)
    }

    pub fn render(self) -> RenderKind {
        match self {
            EventType::SchoolHoliday
            | EventType::DaycareClosure
            | EventType::Birthday
            | EventType::PublicHoliday => RenderKind::Tint,
            _ => RenderKind::Bar,
        }
    }

    /// Whether the type makes a day "busy" for free-period search (§5.5).
    pub fn makes_busy(self) -> bool {
        matches!(
            self,
            EventType::FamilyGathering
                | EventType::FamilyHome
                | EventType::Trip
                | EventType::Celebration
                | EventType::Unclassified
        )
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// `TypeInfo` of `GET /api/config` (§7).
#[derive(Debug, Clone, Serialize)]
pub struct TypeInfo {
    pub code: &'static str,
    pub label: &'static str,
    pub label_plural: &'static str,
    /// `None` for `unclassified`.
    pub calendar: Option<CalendarKind>,
    pub writable: bool,
    pub render: RenderKind,
}

impl TypeInfo {
    pub fn all() -> Vec<TypeInfo> {
        EventType::ALL
            .into_iter()
            .map(|t| TypeInfo {
                code: t.code(),
                label: t.label(),
                label_plural: t.label_plural(),
                calendar: t.home_calendar(),
                writable: t.writable(),
                render: t.render(),
            })
            .collect()
    }
}

/// Origin of an event (`fy_source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Source {
    #[serde(rename = "manual")]
    #[default]
    Manual,
    #[serde(rename = "import:education")]
    ImportEducation,
    #[serde(rename = "import:public_holidays")]
    ImportPublicHolidays,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Manual => "manual",
            Source::ImportEducation => "import:education",
            Source::ImportPublicHolidays => "import:public_holidays",
        }
    }

    pub fn parse(s: &str) -> Source {
        match s {
            "import:education" => Source::ImportEducation,
            "import:public_holidays" => Source::ImportPublicHolidays,
            _ => Source::Manual,
        }
    }

    pub fn is_import(self) -> bool {
        self != Source::Manual
    }
}

/// `<cal>:<googleId>` identifier (§4.7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventId {
    pub calendar: CalendarKind,
    pub google_id: String,
}

impl EventId {
    pub fn new(calendar: CalendarKind, google_id: impl Into<String>) -> Self {
        EventId {
            calendar,
            google_id: google_id.into(),
        }
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.calendar.prefix(), self.google_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Identifiant d’événement invalide.")]
pub struct ParseEventIdError;

impl FromStr for EventId {
    type Err = ParseEventIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (cal, gid) = s.split_once(':').ok_or(ParseEventIdError)?;
        let calendar = CalendarKind::from_prefix(cal).ok_or(ParseEventIdError)?;
        if gid.is_empty() {
            return Err(ParseEventIdError);
        }
        Ok(EventId {
            calendar,
            google_id: gid.to_string(),
        })
    }
}

impl Serialize for EventId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for EventId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Serde helpers for `Option<NaiveTime>` as `"HH:MM"`.
pub mod hm {
    use chrono::NaiveTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<NaiveTime>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(t) => s.serialize_str(&t.format("%H:%M").to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<NaiveTime>, D::Error> {
        let v = Option::<String>::deserialize(d)?;
        match v {
            None => Ok(None),
            Some(s) => NaiveTime::parse_from_str(&s, "%H:%M")
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// The normalized event (§5.1), as exposed by the API and MCP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub series_id: Option<EventId>,
    pub calendar: CalendarKind,
    #[serde(rename = "type")]
    pub kind: EventType,
    pub title: String,
    /// Inclusive first day.
    pub start: NaiveDate,
    /// Inclusive last day (>= start).
    pub end: NaiveDate,
    #[serde(with = "hm", default)]
    pub start_time: Option<NaiveTime>,
    #[serde(with = "hm", default)]
    pub end_time: Option<NaiveTime>,
    /// Empty = whole family.
    pub members: Vec<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub birth_year: Option<i32>,
    pub age: Option<u32>,
    pub source: Source,
    pub read_only: bool,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub google_url: Option<String>,
    /// `fy_source_id` of imported events (not exposed through the API).
    #[serde(skip)]
    pub source_id: Option<String>,
}

impl Event {
    /// Whether the event covers `date` (both ends inclusive).
    pub fn covers(&self, date: NaiveDate) -> bool {
        self.start <= date && date <= self.end
    }

    /// Whether the event overlaps `[from, to]` (inclusive).
    pub fn overlaps(&self, from: NaiveDate, to: NaiveDate) -> bool {
        self.start <= to && self.end >= from
    }

    /// Empty member list means the whole family.
    pub fn involves_any(&self, wanted: &[String]) -> bool {
        self.members.is_empty() || self.members.iter().any(|m| wanted.contains(m))
    }

    pub fn is_recurring(&self) -> bool {
        self.series_id.is_some()
    }

    /// Number of days covered.
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days() + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_id_mapping_round_trips_for_all_typed_kinds() {
        for t in EventType::ALL {
            match t.color_id() {
                Some(id) => assert_eq!(EventType::from_color_id(Some(id)), t),
                None => assert_eq!(t, EventType::Unclassified),
            }
        }
    }

    #[test]
    fn color_ids_match_the_spec_table() {
        let expected = [
            ("5", EventType::SchoolHoliday),
            ("4", EventType::DaycareClosure),
            ("2", EventType::Birthday),
            ("8", EventType::PublicHoliday),
            ("11", EventType::FamilyGathering),
            ("10", EventType::FamilyHome),
            ("9", EventType::Trip),
            ("3", EventType::Celebration),
        ];
        for (id, t) in expected {
            assert_eq!(EventType::from_color_id(Some(id)), t, "colorId {id}");
        }
    }

    #[test]
    fn missing_or_unmapped_color_is_unclassified() {
        assert_eq!(EventType::from_color_id(None), EventType::Unclassified);
        for id in ["1", "6", "7", "12", "", "abc"] {
            assert_eq!(
                EventType::from_color_id(Some(id)),
                EventType::Unclassified,
                "{id}"
            );
        }
    }

    #[test]
    fn each_color_id_is_used_by_one_type() {
        let mut seen = std::collections::HashSet::new();
        for t in EventType::ALL {
            if let Some(id) = t.color_id() {
                assert!(seen.insert(id), "duplicate colorId {id}");
            }
        }
    }

    #[test]
    fn writable_and_calendars() {
        assert!(!EventType::SchoolHoliday.writable());
        assert!(!EventType::PublicHoliday.writable());
        assert!(!EventType::Unclassified.writable());
        assert_eq!(EventType::WRITABLE.len(), 6);
        assert_eq!(
            EventType::Birthday.home_calendar(),
            Some(CalendarKind::Reperes)
        );
        assert_eq!(EventType::Trip.home_calendar(), Some(CalendarKind::Famille));
        assert_eq!(EventType::Unclassified.home_calendar(), None);
    }

    #[test]
    fn codes_round_trip() {
        for t in EventType::ALL {
            assert_eq!(EventType::from_code(t.code()), Some(t));
        }
        assert_eq!(EventType::from_code("nope"), None);
    }

    #[test]
    fn event_id_round_trip() {
        let id: EventId = "f:abc123".parse().unwrap();
        assert_eq!(id.calendar, CalendarKind::Famille);
        assert_eq!(id.to_string(), "f:abc123");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"f:abc123\"");
        // Google instance ids contain underscores, not colons, but keep the first colon only.
        let id: EventId = "r:abc_20260101".parse().unwrap();
        assert_eq!(id.google_id, "abc_20260101");
        assert!("x:abc".parse::<EventId>().is_err());
        assert!("abc".parse::<EventId>().is_err());
        assert!("r:".parse::<EventId>().is_err());
    }

    #[test]
    fn source_strings() {
        for s in [
            Source::Manual,
            Source::ImportEducation,
            Source::ImportPublicHolidays,
        ] {
            assert_eq!(Source::parse(s.as_str()), s);
        }
        assert_eq!(Source::parse("whatever"), Source::Manual);
        assert_eq!(
            serde_json::to_string(&Source::ImportEducation).unwrap(),
            "\"import:education\""
        );
    }
}
