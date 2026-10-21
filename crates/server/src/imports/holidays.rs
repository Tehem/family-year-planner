//! Public holidays of metropolitan France (§6.2), computed locally.

use annee_domain::dates::public_holidays;
use annee_domain::{EventType, Source};
use annee_gcal::ImportedEvent;

pub fn public_holiday_events(year: i32) -> Vec<ImportedEvent> {
    public_holidays(year)
        .into_iter()
        .map(|(date, title)| ImportedEvent {
            kind: EventType::PublicHoliday,
            title: title.to_string(),
            start: date,
            end: date,
            source: Source::ImportPublicHolidays,
            source_id: format!("ferie:{date}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_holidays_with_stable_ids() {
        let v = public_holiday_events(2026);
        assert_eq!(v.len(), 11);
        assert!(
            v.iter()
                .any(|e| e.source_id == "ferie:2026-05-14" && e.title == "Ascension")
        );
        assert!(
            v.iter()
                .all(|e| e.start == e.end && e.kind == EventType::PublicHoliday)
        );
    }
}
