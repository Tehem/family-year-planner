//! The type catalog (§19.3): names, colors and custom types on top of the built-in types.
//!
//! The default catalog is the table of §4.2. The effective catalog is the defaults overridden by
//! what the family configured; it is the only place that maps a Google `colorId` to a type.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::lang::Lang;
use crate::types::{CalendarKind, EventType, MAX_CUSTOM, RenderKind};

/// The 11 event colors of Google Calendar (`colorId` 1..=11).
pub const COLOR_IDS: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"];
/// Longest type name, in characters.
pub const MAX_NAME: usize = 60;

/// Name of a Google color: `(colorId, French, English)`.
pub const COLOR_NAMES: [(&str, &str, &str); 11] = [
    ("1", "Lavande", "Lavender"),
    ("2", "Sauge", "Sage"),
    ("3", "Raisin", "Grape"),
    ("4", "Flamant", "Flamingo"),
    ("5", "Banane", "Banana"),
    ("6", "Mandarine", "Tangerine"),
    ("7", "Paon", "Peacock"),
    ("8", "Graphite", "Graphite"),
    ("9", "Myrtille", "Blueberry"),
    ("10", "Basilic", "Basil"),
    ("11", "Tomate", "Tomato"),
];

/// Singular and plural name in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Names {
    pub singular: String,
    pub plural: String,
}

/// Names of a type in every language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Labels {
    pub fr: Names,
    pub en: Names,
}

impl Labels {
    pub fn get(&self, lang: Lang) -> &Names {
        match lang {
            Lang::Fr => &self.fr,
            Lang::En => &self.en,
        }
    }

    fn defaults(kind: EventType) -> Labels {
        let names = |lang| Names {
            singular: kind.default_label(lang).to_string(),
            plural: kind.default_label_plural(lang).to_string(),
        };
        Labels {
            fr: names(Lang::Fr),
            en: names(Lang::En),
        }
    }
}

/// One entry of the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    pub kind: EventType,
    /// Google `colorId`; `None` only for `Unclassified`.
    pub color_id: Option<String>,
    pub labels: Labels,
}

/// `TypeInfo` of `GET /api/config` (§7, §19.7), in one language.
#[derive(Debug, Clone, Serialize)]
pub struct TypeInfo {
    pub code: &'static str,
    pub label: String,
    pub label_plural: String,
    /// `None` for `unclassified`.
    pub calendar: Option<CalendarKind>,
    pub writable: bool,
    pub render: RenderKind,
    pub color_id: Option<String>,
    pub custom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    UnknownType(String),
    MissingType(&'static str),
    Duplicate(&'static str),
    NameRequired(&'static str),
    NameTooLong(&'static str),
    InvalidColor(String),
    ColorRequired(&'static str),
    ColorForbidden,
    ColorUsedTwice {
        color_id: String,
        first: &'static str,
        second: &'static str,
    },
    TooManyCustom,
}

impl CatalogError {
    pub fn message(&self, lang: Lang) -> String {
        let t = |fr: &str, en: &str| lang.pick(fr, en).to_string();
        match self {
            CatalogError::UnknownType(c) => t(
                &format!("Catégorie inconnue : {c}."),
                &format!("Unknown category: {c}."),
            ),
            CatalogError::MissingType(c) => t(
                &format!("La catégorie {c} est absente de la liste."),
                &format!("The {c} category is missing from the list."),
            ),
            CatalogError::Duplicate(c) => t(
                &format!("La catégorie {c} apparaît deux fois."),
                &format!("The {c} category appears twice."),
            ),
            CatalogError::NameRequired(c) => t(
                &format!(
                    "Donnez un nom à la catégorie {c} (français et anglais, singulier et pluriel)."
                ),
                &format!("Name the {c} category (French and English, singular and plural)."),
            ),
            CatalogError::NameTooLong(c) => t(
                &format!(
                    "Un nom de la catégorie {c} est trop long ({MAX_NAME} caractères maximum)."
                ),
                &format!("A name of the {c} category is too long ({MAX_NAME} characters maximum)."),
            ),
            CatalogError::InvalidColor(id) => t(
                &format!("Couleur inconnue : {id}."),
                &format!("Unknown color: {id}."),
            ),
            CatalogError::ColorRequired(c) => t(
                &format!("Choisissez une couleur pour la catégorie {c}."),
                &format!("Choose a color for the {c} category."),
            ),
            CatalogError::ColorForbidden => t(
                "La catégorie « Non classé » n’a pas de couleur.",
                "The “Unclassified” category has no color.",
            ),
            CatalogError::ColorUsedTwice {
                color_id,
                first,
                second,
            } => {
                let name = |l: Lang| {
                    COLOR_NAMES
                        .iter()
                        .find(|(id, _, _)| id == color_id)
                        .map_or("?", |(_, fr, en)| l.pick(fr, en))
                };
                t(
                    &format!(
                        "La couleur {} est utilisée par {first} et {second} : une couleur ne peut servir qu’à une catégorie.",
                        name(lang)
                    ),
                    &format!(
                        "The color {} is used by both {first} and {second}: a color can only serve one category.",
                        name(lang)
                    ),
                )
            }
            CatalogError::TooManyCustom => t(
                &format!(
                    "{MAX_CUSTOM} catégories ajoutées au maximum : Google Agenda n’offre que 11 couleurs."
                ),
                &format!(
                    "At most {MAX_CUSTOM} added categories: Google Calendar only offers 11 colors."
                ),
            ),
        }
    }
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message(Lang::Fr))
    }
}

impl std::error::Error for CatalogError {}

/// The validated set of types. Built-ins first (in the order of §4.2), then custom types by slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeCatalog {
    defs: Vec<TypeDef>,
}

impl Default for TypeCatalog {
    fn default() -> Self {
        let defs = EventType::ALL
            .into_iter()
            .map(|kind| TypeDef {
                kind,
                color_id: kind.default_color_id().map(String::from),
                labels: Labels::defaults(kind),
            })
            .collect();
        TypeCatalog { defs }
    }
}

fn check_name(kind: EventType, n: &Names) -> Result<(), CatalogError> {
    for v in [&n.singular, &n.plural] {
        let v = v.trim();
        if v.is_empty() {
            return Err(CatalogError::NameRequired(kind.code()));
        }
        if v.chars().count() > MAX_NAME {
            return Err(CatalogError::NameTooLong(kind.code()));
        }
    }
    Ok(())
}

impl TypeCatalog {
    /// Validates a complete catalog: every built-in once, custom types in valid slots (≤ 3),
    /// names set, colors valid and unique.
    pub fn new(defs: Vec<TypeDef>) -> Result<Self, CatalogError> {
        let mut by_kind: BTreeMap<EventType, TypeDef> = BTreeMap::new();
        for mut d in defs {
            if let EventType::Custom(slot) = d.kind
                && !(1..=MAX_CUSTOM).contains(&slot)
            {
                return Err(CatalogError::TooManyCustom);
            }
            for lang in Lang::ALL {
                let names = match lang {
                    Lang::Fr => &mut d.labels.fr,
                    Lang::En => &mut d.labels.en,
                };
                check_name(d.kind, names)?;
                names.singular = names.singular.trim().to_string();
                names.plural = names.plural.trim().to_string();
            }
            match (&d.color_id, d.kind) {
                (Some(_), EventType::Unclassified) => return Err(CatalogError::ColorForbidden),
                (None, EventType::Unclassified) => {}
                (None, k) => return Err(CatalogError::ColorRequired(k.code())),
                (Some(id), _) if !COLOR_IDS.contains(&id.as_str()) => {
                    return Err(CatalogError::InvalidColor(id.clone()));
                }
                _ => {}
            }
            let kind = d.kind;
            if by_kind.insert(kind, d).is_some() {
                return Err(CatalogError::Duplicate(kind.code()));
            }
        }
        for kind in EventType::ALL {
            if !by_kind.contains_key(&kind) {
                return Err(CatalogError::MissingType(kind.code()));
            }
        }
        if by_kind.keys().filter(|k| k.is_custom()).count() > usize::from(MAX_CUSTOM) {
            return Err(CatalogError::TooManyCustom);
        }
        let mut seen: BTreeMap<&str, EventType> = BTreeMap::new();
        for d in by_kind.values() {
            if let Some(id) = d.color_id.as_deref()
                && let Some(prev) = seen.insert(id, d.kind)
            {
                return Err(CatalogError::ColorUsedTwice {
                    color_id: id.to_string(),
                    first: prev.code(),
                    second: d.kind.code(),
                });
            }
        }
        Ok(TypeCatalog {
            defs: by_kind.into_values().collect(),
        })
    }

    /// The defaults with `overrides` replacing (or, for custom types, adding) entries. Invalid
    /// overrides make the whole call fail; callers fall back to [`TypeCatalog::default`].
    pub fn with_overrides(overrides: Vec<TypeDef>) -> Result<Self, CatalogError> {
        let mut map: BTreeMap<EventType, TypeDef> = TypeCatalog::default()
            .defs
            .into_iter()
            .map(|d| (d.kind, d))
            .collect();
        for o in overrides {
            map.insert(o.kind, o);
        }
        TypeCatalog::new(map.into_values().collect())
    }

    pub fn defs(&self) -> &[TypeDef] {
        &self.defs
    }

    pub fn get(&self, kind: EventType) -> Option<&TypeDef> {
        self.defs.iter().find(|d| d.kind == kind)
    }

    pub fn contains(&self, kind: EventType) -> bool {
        self.get(kind).is_some()
    }

    pub fn kinds(&self) -> impl Iterator<Item = EventType> + '_ {
        self.defs.iter().map(|d| d.kind)
    }

    /// Type for a Google `colorId`. Unknown or missing ids are `Unclassified`.
    pub fn from_color_id(&self, color_id: Option<&str>) -> EventType {
        let Some(id) = color_id else {
            return EventType::Unclassified;
        };
        self.defs
            .iter()
            .find(|d| d.color_id.as_deref() == Some(id))
            .map_or(EventType::Unclassified, |d| d.kind)
    }

    /// Google `colorId` written for the type (`None` for `Unclassified`).
    pub fn color_id(&self, kind: EventType) -> Option<&str> {
        self.get(kind).and_then(|d| d.color_id.as_deref())
    }

    pub fn label(&self, kind: EventType, lang: Lang) -> String {
        self.get(kind).map_or_else(
            || kind.code().to_string(),
            |d| d.labels.get(lang).singular.clone(),
        )
    }

    /// Colors not used by any type.
    pub fn free_colors(&self) -> Vec<&'static str> {
        COLOR_IDS
            .into_iter()
            .filter(|id| !self.defs.iter().any(|d| d.color_id.as_deref() == Some(id)))
            .collect()
    }

    /// Types as exposed by `GET /api/config`, in `lang`.
    pub fn infos(&self, lang: Lang) -> Vec<TypeInfo> {
        self.defs
            .iter()
            .map(|d| TypeInfo {
                code: d.kind.code(),
                label: d.labels.get(lang).singular.clone(),
                label_plural: d.labels.get(lang).plural.clone(),
                calendar: d.kind.home_calendar(),
                writable: d.kind.writable(),
                render: d.kind.render(),
                color_id: d.color_id.clone(),
                custom: d.kind.is_custom(),
            })
            .collect()
    }

    /// Per color change between `self` (old) and `target`: `old colorId → new colorId`
    /// (`None` = the color is cleared, the events become unclassified). Types without a change
    /// and unclassified are skipped. Used by the migration of §19.5.
    pub fn color_mapping(&self, target: &TypeCatalog) -> BTreeMap<String, Option<String>> {
        let mut map = BTreeMap::new();
        for old in &self.defs {
            let Some(old_id) = old.color_id.clone() else {
                continue;
            };
            let new_id = target.get(old.kind).and_then(|d| d.color_id.clone());
            if new_id.as_deref() != Some(old_id.as_str()) {
                map.insert(old_id, new_id);
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(slot: u8, color: &str, name: &str) -> TypeDef {
        let n = |s: &str| Names {
            singular: s.to_string(),
            plural: format!("{s}s"),
        };
        TypeDef {
            kind: EventType::Custom(slot),
            color_id: Some(color.to_string()),
            labels: Labels {
                fr: n(name),
                en: n(name),
            },
        }
    }

    #[test]
    fn default_catalog_matches_the_spec_table() {
        let c = TypeCatalog::default();
        assert_eq!(c.defs().len(), 9);
        for (id, kind) in [
            ("5", EventType::SchoolHoliday),
            ("4", EventType::DaycareClosure),
            ("2", EventType::Birthday),
            ("8", EventType::PublicHoliday),
            ("11", EventType::FamilyGathering),
            ("10", EventType::FamilyHome),
            ("9", EventType::Trip),
            ("3", EventType::Celebration),
        ] {
            assert_eq!(c.from_color_id(Some(id)), kind, "colorId {id}");
            assert_eq!(c.color_id(kind), Some(id));
        }
        assert_eq!(c.label(EventType::Trip, Lang::Fr), "Voyage ou vacances");
        assert_eq!(c.label(EventType::Trip, Lang::En), "Trip or vacation");
        assert_eq!(c.free_colors(), vec!["1", "6", "7"]);
    }

    #[test]
    fn missing_or_unmapped_color_is_unclassified() {
        let c = TypeCatalog::default();
        assert_eq!(c.from_color_id(None), EventType::Unclassified);
        for id in ["1", "6", "7", "12", "", "abc"] {
            assert_eq!(c.from_color_id(Some(id)), EventType::Unclassified, "{id}");
        }
    }

    #[test]
    fn custom_type_takes_a_free_color() {
        let c = TypeCatalog::with_overrides(vec![custom(1, "6", "Sport")]).unwrap();
        assert_eq!(c.from_color_id(Some("6")), EventType::Custom(1));
        assert_eq!(c.color_id(EventType::Custom(1)), Some("6"));
        assert_eq!(c.defs().iter().filter(|d| d.kind.is_custom()).count(), 1);
        assert_eq!(c.free_colors(), vec!["1", "7"]);
        let info = c.infos(Lang::En);
        let info = info.iter().find(|i| i.code == "custom_1").unwrap();
        assert!(info.custom && info.writable);
        assert_eq!(info.calendar, Some(CalendarKind::Famille));
    }

    #[test]
    fn at_most_three_custom_types_and_no_duplicate_color() {
        let three = vec![
            custom(1, "1", "A"),
            custom(2, "6", "B"),
            custom(3, "7", "C"),
        ];
        let c = TypeCatalog::with_overrides(three).unwrap();
        assert!(c.free_colors().is_empty());
        assert_eq!(
            TypeCatalog::with_overrides(vec![custom(4, "6", "D")]),
            Err(CatalogError::TooManyCustom)
        );
        assert!(matches!(
            TypeCatalog::with_overrides(vec![custom(1, "9", "Dup")]),
            Err(CatalogError::ColorUsedTwice { .. })
        ));
    }

    #[test]
    fn names_and_colors_are_validated() {
        let mut bad = custom(1, "6", "  ");
        assert_eq!(
            TypeCatalog::with_overrides(vec![bad.clone()]),
            Err(CatalogError::NameRequired("custom_1"))
        );
        bad = custom(1, "6", &"x".repeat(61));
        assert_eq!(
            TypeCatalog::with_overrides(vec![bad]),
            Err(CatalogError::NameTooLong("custom_1"))
        );
        assert_eq!(
            TypeCatalog::with_overrides(vec![custom(1, "12", "A")]),
            Err(CatalogError::InvalidColor("12".into()))
        );
        let mut unclassified = TypeCatalog::default()
            .get(EventType::Unclassified)
            .unwrap()
            .clone();
        unclassified.color_id = Some("6".into());
        assert_eq!(
            TypeCatalog::with_overrides(vec![unclassified]),
            Err(CatalogError::ColorForbidden)
        );
    }

    #[test]
    fn renaming_and_recoloring_a_built_in() {
        let mut trip = TypeCatalog::default().get(EventType::Trip).unwrap().clone();
        trip.color_id = Some("6".into());
        trip.labels.en.singular = "Holiday".into();
        let c = TypeCatalog::with_overrides(vec![trip]).unwrap();
        assert_eq!(c.from_color_id(Some("9")), EventType::Unclassified);
        assert_eq!(c.from_color_id(Some("6")), EventType::Trip);
        assert_eq!(c.label(EventType::Trip, Lang::En), "Holiday");
        assert_eq!(c.label(EventType::Trip, Lang::Fr), "Voyage ou vacances");
    }

    #[test]
    fn color_mapping_handles_swaps_clears_and_untouched_types() {
        let old = TypeCatalog::with_overrides(vec![custom(1, "6", "Sport")]).unwrap();
        let mut trip = old.get(EventType::Trip).unwrap().clone();
        let mut gathering = old.get(EventType::FamilyGathering).unwrap().clone();
        trip.color_id = Some("11".into());
        gathering.color_id = Some("9".into());
        let swapped = TypeCatalog::with_overrides(vec![trip, gathering]).unwrap();
        let swapped = TypeCatalog::new({
            let mut d = swapped.defs().to_vec();
            d.retain(|x| x.kind != EventType::Custom(1));
            d
        })
        .unwrap();
        let m = old.color_mapping(&swapped);
        assert_eq!(m.get("9"), Some(&Some("11".to_string())));
        assert_eq!(m.get("11"), Some(&Some("9".to_string())));
        assert_eq!(
            m.get("6"),
            Some(&None),
            "deleted custom type clears its color"
        );
        assert_eq!(m.len(), 3);
    }
}
