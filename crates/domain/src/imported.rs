//! Titles of imported events are stored in French in Google Calendar and translated when they are
//! displayed (§19.2).

use crate::lang::Lang;
use crate::types::Event;

/// French stored title → English display title.
const EN: [(&str, &str); 17] = [
    ("Jour de l’An", "New Year’s Day"),
    ("Lundi de Pâques", "Easter Monday"),
    ("Fête du Travail", "Labour Day"),
    ("Victoire 1945", "Victory in Europe Day"),
    ("Ascension", "Ascension Day"),
    ("Lundi de Pentecôte", "Whit Monday"),
    ("Fête nationale", "Bastille Day"),
    ("Assomption", "Assumption Day"),
    ("Toussaint", "All Saints’ Day"),
    ("Armistice 1918", "Armistice Day"),
    ("Noël", "Christmas Day"),
    ("Vacances de Noël", "Christmas holidays"),
    ("Vacances d’hiver", "Winter holidays"),
    ("Vacances de printemps", "Spring holidays"),
    ("Vacances d’été", "Summer holidays"),
    ("Vacances de la Toussaint", "All Saints’ holidays"),
    ("Pont de l’Ascension", "Ascension bridge"),
];

/// The title of an imported event in `lang`; unknown titles are returned unchanged.
pub fn translate_title(title: &str, lang: Lang) -> String {
    match lang {
        Lang::Fr => title.to_string(),
        Lang::En => EN
            .iter()
            .find(|(fr, _)| *fr == title)
            .map_or_else(|| title.to_string(), |(_, en)| (*en).to_string()),
    }
}

/// Translate the title of an imported event in place. User-written titles are never touched.
pub fn localize(event: &mut Event, lang: Lang) {
    if event.source.is_import() {
        event.title = translate_title(&event.title, lang);
    }
}

pub fn localize_all(events: &mut [Event], lang: Lang) {
    for e in events {
        localize(e, lang);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::public_holidays;

    #[test]
    fn every_public_holiday_has_an_english_name() {
        for (_, fr) in public_holidays(2026) {
            assert_ne!(translate_title(fr, Lang::En), fr, "{fr}");
        }
    }

    #[test]
    fn french_and_unknown_titles_are_unchanged() {
        assert_eq!(
            translate_title("Vacances d’été", Lang::Fr),
            "Vacances d’été"
        );
        assert_eq!(
            translate_title("Vacances d’été", Lang::En),
            "Summer holidays"
        );
        assert_eq!(translate_title("Carnaval", Lang::En), "Carnaval");
    }
}
