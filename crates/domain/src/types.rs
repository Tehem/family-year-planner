//! Core types: calendars, event types (§4.2), identifiers and the normalized `Event` (§5.1).

use std::fmt;
use std::str::FromStr;

use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::lang::Lang;

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

/// Highest slot number of a custom type (`custom_1` … `custom_3`): Google offers 11 event colors
/// and the 8 colored built-in types use 8 of them (§19.3).
pub const MAX_CUSTOM: u8 = 3;

/// Category of an event. The Google `colorId` → type mapping lives in the
/// [`TypeCatalog`](crate::catalog::TypeCatalog) (§19.3); this enum is the stable identity of a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    /// User-defined type in slot `1..=MAX_CUSTOM` (a Famille bar).
    Custom(u8),
}

impl EventType {
    /// The built-in types (custom types come from the catalog).
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

    /// The six built-in types users can create and edit, in the order of the dialog's select.
    pub const WRITABLE: [EventType; 6] = [
        EventType::Birthday,
        EventType::Celebration,
        EventType::FamilyGathering,
        EventType::FamilyHome,
        EventType::Trip,
        EventType::DaycareClosure,
    ];

    pub fn is_custom(self) -> bool {
        matches!(self, EventType::Custom(_))
    }

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
            EventType::Custom(1) => "custom_1",
            EventType::Custom(2) => "custom_2",
            EventType::Custom(3) => "custom_3",
            EventType::Custom(_) => "custom_invalid",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        // Exactly one spelling per type: `custom_01` and `custom_+1` are not codes.
        match code {
            "custom_1" => return Some(EventType::Custom(1)),
            "custom_2" => return Some(EventType::Custom(2)),
            "custom_3" => return Some(EventType::Custom(3)),
            _ => {}
        }
        EventType::ALL.into_iter().find(|t| t.code() == code)
    }

    /// Default Google `colorId` of a built-in type (`None` for `Unclassified` and custom types).
    pub fn default_color_id(self) -> Option<&'static str> {
        match self {
            EventType::SchoolHoliday => Some("5"),
            EventType::DaycareClosure => Some("4"),
            EventType::Birthday => Some("2"),
            EventType::PublicHoliday => Some("8"),
            EventType::FamilyGathering => Some("11"),
            EventType::FamilyHome => Some("10"),
            EventType::Trip => Some("9"),
            EventType::Celebration => Some("3"),
            EventType::Unclassified | EventType::Custom(_) => None,
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
            | EventType::Celebration
            | EventType::Custom(_) => Some(CalendarKind::Famille),
            EventType::Unclassified => None,
        }
    }

    /// Default singular name of a built-in type (empty for custom types).
    pub fn default_label(self, lang: Lang) -> &'static str {
        let (fr, en) = match self {
            EventType::SchoolHoliday => ("Vacances scolaires", "School holiday"),
            EventType::DaycareClosure => (
                "Fermeture de la crèche ou du centre de loisirs",
                "Daycare or leisure-centre closure",
            ),
            EventType::Birthday => ("Anniversaire", "Birthday"),
            EventType::PublicHoliday => ("Jour férié", "Public holiday"),
            EventType::FamilyGathering => ("Réunion de famille", "Family gathering"),
            EventType::FamilyHome => ("Séjour en maison de famille", "Stay at a family home"),
            EventType::Trip => ("Voyage ou vacances", "Trip or vacation"),
            EventType::Celebration => ("Fête ou cérémonie", "Party or ceremony"),
            EventType::Unclassified => ("Non classé", "Unclassified"),
            EventType::Custom(_) => ("", ""),
        };
        match lang {
            Lang::Fr => fr,
            Lang::En => en,
        }
    }

    /// Default plural name (legend) of a built-in type (empty for custom types).
    pub fn default_label_plural(self, lang: Lang) -> &'static str {
        let (fr, en) = match self {
            EventType::SchoolHoliday => ("Vacances scolaires", "School holidays"),
            EventType::DaycareClosure => ("Crèche / loisirs fermés", "Daycare / leisure closed"),
            EventType::Birthday => ("Anniversaires", "Birthdays"),
            EventType::PublicHoliday => ("Jours fériés", "Public holidays"),
            EventType::FamilyGathering => ("Réunions de famille", "Family gatherings"),
            EventType::FamilyHome => ("Maisons de famille", "Family homes"),
            EventType::Trip => ("Voyages & vacances", "Trips & vacations"),
            EventType::Celebration => ("Fêtes & cérémonies", "Parties & ceremonies"),
            EventType::Unclassified => ("Non classés", "Unclassified"),
            EventType::Custom(_) => ("", ""),
        };
        match lang {
            Lang::Fr => fr,
            Lang::En => en,
        }
    }

    /// Users can create events of this type (and imported types cannot be edited).
    pub fn writable(self) -> bool {
        self.is_custom() || EventType::WRITABLE.contains(&self)
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
                | EventType::Custom(_)
        )
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl Serialize for EventType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for EventType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        EventType::from_code(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown event type {s:?}")))
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
    fn default_color_ids_match_the_spec_table() {
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
            assert_eq!(t.default_color_id(), Some(id), "{t}");
        }
        assert_eq!(EventType::Unclassified.default_color_id(), None);
        assert_eq!(EventType::Custom(1).default_color_id(), None);
    }

    #[test]
    fn each_default_color_id_is_used_by_one_type() {
        let mut seen = std::collections::HashSet::new();
        for t in EventType::ALL {
            if let Some(id) = t.default_color_id() {
                assert!(seen.insert(id), "duplicate colorId {id}");
            }
        }
    }

    #[test]
    fn writable_and_calendars() {
        assert!(!EventType::SchoolHoliday.writable());
        assert!(!EventType::PublicHoliday.writable());
        assert!(!EventType::Unclassified.writable());
        assert!(EventType::Custom(2).writable());
        assert_eq!(EventType::WRITABLE.len(), 6);
        assert_eq!(
            EventType::Birthday.home_calendar(),
            Some(CalendarKind::Reperes)
        );
        assert_eq!(EventType::Trip.home_calendar(), Some(CalendarKind::Famille));
        assert_eq!(
            EventType::Custom(1).home_calendar(),
            Some(CalendarKind::Famille)
        );
        assert_eq!(EventType::Unclassified.home_calendar(), None);
    }

    #[test]
    fn codes_round_trip() {
        for t in EventType::ALL {
            assert_eq!(EventType::from_code(t.code()), Some(t));
        }
        for slot in 1..=MAX_CUSTOM {
            let t = EventType::Custom(slot);
            assert_eq!(EventType::from_code(t.code()), Some(t));
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(serde_json::from_str::<EventType>(&json).unwrap(), t);
        }
        assert_eq!(EventType::from_code("nope"), None);
        assert_eq!(EventType::from_code("custom_0"), None);
        assert_eq!(EventType::from_code("custom_4"), None);
        assert_eq!(EventType::from_code("custom_"), None);
        for spelling in [
            "custom_01",
            "custom_+1",
            "custom_001",
            "custom_1 ",
            "Custom_1",
        ] {
            assert_eq!(EventType::from_code(spelling), None, "{spelling}");
        }
    }

    #[test]
    fn default_labels_exist_in_both_languages() {
        for t in EventType::ALL {
            for lang in [Lang::Fr, Lang::En] {
                assert!(!t.default_label(lang).is_empty(), "{t} {lang:?}");
                assert!(!t.default_label_plural(lang).is_empty(), "{t} {lang:?}");
            }
        }
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
