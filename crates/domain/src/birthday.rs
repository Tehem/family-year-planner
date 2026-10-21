//! Birthdays: yearly recurrence rules and age computation (§4.6).

use chrono::{Datelike, NaiveDate};

use crate::dates::is_leap_year;

/// RRULE for a yearly birthday on `date`. A 29 February birthday falls on the last day of
/// February in non-leap years.
pub fn recurrence_rule(date: NaiveDate) -> String {
    if date.month() == 2 && date.day() == 29 {
        "RRULE:FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=-1".to_string()
    } else {
        "RRULE:FREQ=YEARLY".to_string()
    }
}

/// First occurrence of the series: the birth date when the birth year is known, otherwise the
/// date as chosen by the user. `None` if the birth year makes the date impossible.
pub fn first_occurrence(date: NaiveDate, birth_year: Option<i32>) -> Option<NaiveDate> {
    match birth_year {
        Some(y) => NaiveDate::from_ymd_opt(y, date.month(), date.day()),
        None => Some(date),
    }
}

/// Whether a (month, day) birthday can exist in `birth_year`.
pub fn birth_date_possible(month: u32, day: u32, birth_year: i32) -> bool {
    !(month == 2 && day == 29 && !is_leap_year(birth_year))
}

/// Age for an occurrence: occurrence year − birth year, `None` when unknown or not positive.
pub fn age_at(birth_year: Option<i32>, occurrence_year: i32) -> Option<u32> {
    let age = occurrence_year - birth_year?;
    u32::try_from(age).ok().filter(|a| *a > 0)
}

/// `8 ans`, `1 an`.
pub fn age_label(age: u32) -> String {
    if age == 1 {
        "1 an".to_string()
    } else {
        format!("{age} ans")
    }
}

/// Title shown to users: `Anniversaire de Sacha (8 ans)` when the age is known.
pub fn display_title(title: &str, age: Option<u32>) -> String {
    match age {
        Some(a) => format!("{title} ({})", age_label(a)),
        None => title.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::ymd;

    #[test]
    fn plain_yearly_rule() {
        assert_eq!(recurrence_rule(ymd(2026, 3, 12)), "RRULE:FREQ=YEARLY");
    }

    #[test]
    fn leap_day_rule_uses_last_day_of_february() {
        assert_eq!(
            recurrence_rule(ymd(2028, 2, 29)),
            "RRULE:FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=-1"
        );
    }

    #[test]
    fn first_occurrence_uses_birth_year_when_known() {
        assert_eq!(
            first_occurrence(ymd(2026, 3, 12), Some(2018)),
            Some(ymd(2018, 3, 12))
        );
        assert_eq!(
            first_occurrence(ymd(2026, 3, 12), None),
            Some(ymd(2026, 3, 12))
        );
        // 29 February in a non-leap birth year is impossible.
        assert_eq!(first_occurrence(ymd(2028, 2, 29), Some(2019)), None);
        assert_eq!(
            first_occurrence(ymd(2028, 2, 29), Some(2020)),
            Some(ymd(2020, 2, 29))
        );
        assert!(!birth_date_possible(2, 29, 2019));
        assert!(birth_date_possible(2, 29, 2020));
        assert!(birth_date_possible(2, 28, 2019));
    }

    #[test]
    fn ages() {
        assert_eq!(age_at(Some(2018), 2026), Some(8));
        assert_eq!(age_at(Some(2025), 2026), Some(1));
        assert_eq!(age_at(Some(2026), 2026), None);
        assert_eq!(age_at(Some(2027), 2026), None);
        assert_eq!(age_at(None, 2026), None);
    }

    #[test]
    fn titles_with_age() {
        assert_eq!(
            display_title("Anniversaire de Sacha", Some(8)),
            "Anniversaire de Sacha (8 ans)"
        );
        assert_eq!(
            display_title("Anniversaire de Noé", Some(1)),
            "Anniversaire de Noé (1 an)"
        );
        assert_eq!(
            display_title("Anniversaire de Camille", None),
            "Anniversaire de Camille"
        );
    }
}
