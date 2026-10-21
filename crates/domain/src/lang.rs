//! Interface languages (§19.2).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Language of the interface and of server messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Fr, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// Parses `fr` / `en` (case-insensitive, region suffix ignored: `en-GB` → `en`).
    pub fn parse(s: &str) -> Option<Lang> {
        let primary = s.trim().split(['-', '_']).next()?.to_ascii_lowercase();
        match primary.as_str() {
            "fr" => Some(Lang::Fr),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    /// Picks the text of this language.
    pub fn pick<'a>(self, fr: &'a str, en: &'a str) -> &'a str {
        match self {
            Lang::Fr => fr,
            Lang::En => en,
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_regions_and_case() {
        assert_eq!(Lang::parse("fr"), Some(Lang::Fr));
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("en-GB"), Some(Lang::En));
        assert_eq!(Lang::parse("fr_FR"), Some(Lang::Fr));
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::parse(""), None);
    }

    #[test]
    fn serde_is_lowercase() {
        assert_eq!(serde_json::to_string(&Lang::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::from_str::<Lang>("\"fr\"").unwrap(), Lang::Fr);
    }
}
