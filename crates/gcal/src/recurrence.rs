//! Yearly recurrence helpers shared by both stores.

use annee_domain::dates::{is_leap_year, ymd};
use chrono::{Datelike, Duration, NaiveDate};

/// Google instance id of an all-day occurrence: `<seriesId>_<yyyymmdd>`.
pub fn instance_id(series_google_id: &str, date: NaiveDate) -> String {
    format!("{series_google_id}_{}", date.format("%Y%m%d"))
}

/// Occurrence of a yearly series in `year` (a 29 February birthday falls on 28 February in
/// non-leap years, like `BYMONTHDAY=-1`). `None` before the series starts.
pub fn yearly_occurrence(series_start: NaiveDate, year: i32) -> Option<NaiveDate> {
    if year < series_start.year() {
        return None;
    }
    let (m, d) = (series_start.month(), series_start.day());
    if m == 2 && d == 29 && !is_leap_year(year) {
        return Some(ymd(year, 2, 28));
    }
    NaiveDate::from_ymd_opt(year, m, d)
}

/// New first occurrence of a birthday series after an edit.
///
/// With a birth year the series starts on the birth date. Without one it keeps the year it
/// already started in (so the birthday does not vanish from earlier years) and takes the new
/// day and month; if that year cannot hold the date (29 February), the edited date's year is used.
pub fn birthday_series_start(
    new_date: NaiveDate,
    birth_year: Option<i32>,
    current_series_start: NaiveDate,
) -> NaiveDate {
    let year = birth_year.unwrap_or(current_series_start.year());
    NaiveDate::from_ymd_opt(year, new_date.month(), new_date.day()).unwrap_or(new_date)
}

/// Shift a date by a number of days (used to move a non-birthday series).
pub fn shift(date: NaiveDate, days: i64) -> NaiveDate {
    date + Duration::days(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_ids() {
        assert_eq!(instance_id("abc", ymd(2026, 3, 12)), "abc_20260312");
    }

    #[test]
    fn yearly_occurrences() {
        let s = ymd(2018, 3, 12);
        assert_eq!(yearly_occurrence(s, 2017), None);
        assert_eq!(yearly_occurrence(s, 2018), Some(ymd(2018, 3, 12)));
        assert_eq!(yearly_occurrence(s, 2030), Some(ymd(2030, 3, 12)));
    }

    #[test]
    fn leap_day_birthday_falls_on_28_february() {
        let s = ymd(2020, 2, 29);
        assert_eq!(yearly_occurrence(s, 2021), Some(ymd(2021, 2, 28)));
        assert_eq!(yearly_occurrence(s, 2024), Some(ymd(2024, 2, 29)));
        assert_eq!(yearly_occurrence(s, 2100), Some(ymd(2100, 2, 28)));
    }

    #[test]
    fn birthday_start_rules() {
        // Birth year known: the birth date.
        assert_eq!(
            birthday_series_start(ymd(2026, 3, 13), Some(2018), ymd(1900, 3, 12)),
            ymd(2018, 3, 13)
        );
        // Unknown: keep the series' start year.
        assert_eq!(
            birthday_series_start(ymd(2026, 3, 13), None, ymd(2024, 3, 12)),
            ymd(2024, 3, 13)
        );
        // 29 February cannot start in a non-leap year: fall back to the edited date.
        assert_eq!(
            birthday_series_start(ymd(2028, 2, 29), None, ymd(2025, 3, 12)),
            ymd(2028, 2, 29)
        );
    }

    #[test]
    fn shifting() {
        assert_eq!(shift(ymd(2026, 12, 31), 1), ymd(2027, 1, 1));
        assert_eq!(shift(ymd(2026, 1, 1), -1), ymd(2025, 12, 31));
    }
}
