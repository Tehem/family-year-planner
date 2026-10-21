//! Date helpers: inclusive/exclusive ends, timed events, Easter and French public holidays.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, TimeZone};
use chrono_tz::Tz;

/// Build a date, panicking on invalid input (only used with known-good constants).
pub fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

/// Google all-day events use an exclusive `end.date`; the app uses inclusive ends.
pub fn inclusive_from_exclusive(end_exclusive: NaiveDate) -> NaiveDate {
    end_exclusive - Duration::days(1)
}

/// Inverse of [`inclusive_from_exclusive`].
pub fn exclusive_from_inclusive(end_inclusive: NaiveDate) -> NaiveDate {
    end_inclusive + Duration::days(1)
}

/// Local dates and times covered by a timed event (§4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedSpan {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
}

/// A timed event covers every local date it touches: the first day is the local date of
/// `start`, the last day is the local date of `end − 1 second` (so an event ending exactly at
/// midnight does not spill on the next day).
pub fn timed_span(start: DateTime<FixedOffset>, end: DateTime<FixedOffset>, tz: Tz) -> TimedSpan {
    let s = start.with_timezone(&tz);
    let e = end.with_timezone(&tz);
    let last = (end - Duration::seconds(1)).with_timezone(&tz).date_naive();
    let first = s.date_naive();
    TimedSpan {
        start: first,
        end: last.max(first),
        start_time: s.time(),
        end_time: e.time(),
    }
}

/// Midnight-anchored RFC 3339 instants for a local date, for Google `timeMin` / `timeMax`.
pub fn local_midnight_utc(date: NaiveDate, tz: Tz) -> DateTime<chrono::Utc> {
    let naive = date.and_hms_opt(0, 0, 0).expect("midnight");
    match tz.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => dt.with_timezone(&chrono::Utc),
        chrono::LocalResult::Ambiguous(a, _) => a.with_timezone(&chrono::Utc),
        // A DST gap at midnight: use one hour later.
        chrono::LocalResult::None => tz
            .from_local_datetime(&(naive + Duration::hours(1)))
            .earliest()
            .expect("valid local time")
            .with_timezone(&chrono::Utc),
    }
}

/// Every date from `start` to `end`, inclusive.
pub fn each_day(start: NaiveDate, end: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    let n = (end - start).num_days() + 1;
    (0..n.max(0)).map(move |i| start + Duration::days(i))
}

/// First and last day of a calendar year.
pub fn year_bounds(year: i32) -> (NaiveDate, NaiveDate) {
    (ymd(year, 1, 1), ymd(year, 12, 31))
}

pub fn is_leap_year(year: i32) -> bool {
    NaiveDate::from_ymd_opt(year, 2, 29).is_some()
}

/// Easter Sunday (anonymous Gregorian algorithm, as in the prototype).
pub fn easter(year: i32) -> NaiveDate {
    let y = year;
    let a = y % 19;
    let b = y / 100;
    let c = y % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;
    ymd(y, month as u32, day as u32)
}

/// French public holidays of metropolitan France with their titles (§6.2), sorted by date.
pub fn public_holidays(year: i32) -> Vec<(NaiveDate, &'static str)> {
    let e = easter(year);
    let mut v = vec![
        (ymd(year, 1, 1), "Jour de l’An"),
        (e + Duration::days(1), "Lundi de Pâques"),
        (ymd(year, 5, 1), "Fête du Travail"),
        (ymd(year, 5, 8), "Victoire 1945"),
        (e + Duration::days(39), "Ascension"),
        (e + Duration::days(50), "Lundi de Pentecôte"),
        (ymd(year, 7, 14), "Fête nationale"),
        (ymd(year, 8, 15), "Assomption"),
        (ymd(year, 11, 1), "Toussaint"),
        (ymd(year, 11, 11), "Armistice 1918"),
        (ymd(year, 12, 25), "Noël"),
    ];
    v.sort_by_key(|(d, _)| *d);
    v
}

/// Calendar year in which the school year containing `date` starts (September rule).
pub fn school_year_start(date: NaiveDate) -> i32 {
    if date.month() >= 9 {
        date.year()
    } else {
        date.year() - 1
    }
}

/// `2025-2026` for a school year starting in 2025.
pub fn school_year_label(start_year: i32) -> String {
    format!("{}-{}", start_year, start_year + 1)
}

/// Monday-based weekday index (Monday = 0).
pub fn weekday_index(date: NaiveDate) -> u32 {
    date.weekday().num_days_from_monday()
}

pub fn is_weekend(date: NaiveDate) -> bool {
    weekday_index(date) >= 5
}

/// French weekday name, lowercase (`lundi`).
pub fn weekday_fr(date: NaiveDate) -> &'static str {
    [
        "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
    ][weekday_index(date) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Paris;

    fn dt(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn exclusive_inclusive_round_trip() {
        let excl = ymd(2026, 3, 1);
        let incl = inclusive_from_exclusive(excl);
        assert_eq!(incl, ymd(2026, 2, 28));
        assert_eq!(exclusive_from_inclusive(incl), excl);
        // Across a year boundary and a leap day.
        assert_eq!(inclusive_from_exclusive(ymd(2027, 1, 1)), ymd(2026, 12, 31));
        assert_eq!(exclusive_from_inclusive(ymd(2028, 2, 28)), ymd(2028, 2, 29));
    }

    #[test]
    fn timed_event_same_day() {
        let s = timed_span(
            dt("2026-03-14T15:00:00+01:00"),
            dt("2026-03-14T23:00:00+01:00"),
            Paris,
        );
        assert_eq!(s.start, ymd(2026, 3, 14));
        assert_eq!(s.end, ymd(2026, 3, 14));
        assert_eq!(s.start_time.to_string(), "15:00:00");
        assert_eq!(s.end_time.to_string(), "23:00:00");
    }

    #[test]
    fn timed_event_across_midnight_covers_both_days() {
        let s = timed_span(
            dt("2026-03-14T22:00:00+01:00"),
            dt("2026-03-15T02:00:00+01:00"),
            Paris,
        );
        assert_eq!(s.start, ymd(2026, 3, 14));
        assert_eq!(s.end, ymd(2026, 3, 15));
    }

    #[test]
    fn timed_event_ending_at_midnight_does_not_spill() {
        let s = timed_span(
            dt("2026-03-14T20:00:00+01:00"),
            dt("2026-03-15T00:00:00+01:00"),
            Paris,
        );
        assert_eq!(s.end, ymd(2026, 3, 14));
    }

    #[test]
    fn timed_event_uses_local_dates_not_utc() {
        // 23:30 in Paris (UTC+1) is 22:30 UTC the same day; 00:30 Paris is 23:30 UTC the day before.
        let s = timed_span(
            dt("2026-03-14T23:30:00Z"),
            dt("2026-03-15T01:30:00Z"),
            Paris,
        );
        assert_eq!(s.start, ymd(2026, 3, 15));
        assert_eq!(s.start_time.to_string(), "00:30:00");
        assert_eq!(s.end, ymd(2026, 3, 15));
    }

    #[test]
    fn timed_event_across_spring_dst_change() {
        // 2026-03-29: clocks go from 02:00 to 03:00 in Paris.
        let s = timed_span(
            dt("2026-03-28T22:00:00+01:00"),
            dt("2026-03-29T08:00:00+02:00"),
            Paris,
        );
        assert_eq!(s.start, ymd(2026, 3, 28));
        assert_eq!(s.end, ymd(2026, 3, 29));
        assert_eq!(s.end_time.to_string(), "08:00:00");
        // An event ending at local midnight after the change.
        let s = timed_span(
            dt("2026-03-29T10:00:00+02:00"),
            dt("2026-03-30T00:00:00+02:00"),
            Paris,
        );
        assert_eq!(s.end, ymd(2026, 3, 29));
    }

    #[test]
    fn timed_event_across_autumn_dst_change() {
        // 2026-10-25: clocks go from 03:00 to 02:00 in Paris.
        let s = timed_span(
            dt("2026-10-24T23:00:00+02:00"),
            dt("2026-10-25T01:30:00+01:00"),
            Paris,
        );
        assert_eq!(s.start, ymd(2026, 10, 24));
        assert_eq!(s.end, ymd(2026, 10, 25));
        // A 25-hour day: an event until 00:00 on the 26th still ends on the 25th.
        let s = timed_span(
            dt("2026-10-25T12:00:00+01:00"),
            dt("2026-10-26T00:00:00+01:00"),
            Paris,
        );
        assert_eq!(s.end, ymd(2026, 10, 25));
    }

    #[test]
    fn zero_length_timed_event_stays_on_its_day() {
        let s = timed_span(
            dt("2026-03-14T00:00:00+01:00"),
            dt("2026-03-14T00:00:00+01:00"),
            Paris,
        );
        assert_eq!(s.start, s.end);
    }

    #[test]
    fn easter_dates() {
        assert_eq!(easter(2024), ymd(2024, 3, 31));
        assert_eq!(easter(2025), ymd(2025, 4, 20));
        assert_eq!(easter(2026), ymd(2026, 4, 5));
        assert_eq!(easter(2027), ymd(2027, 3, 28));
        assert_eq!(easter(2038), ymd(2038, 4, 25));
    }

    #[test]
    fn public_holidays_2026() {
        let h = public_holidays(2026);
        assert_eq!(h.len(), 11);
        let get = |name: &str| h.iter().find(|(_, n)| *n == name).unwrap().0;
        assert_eq!(get("Lundi de Pâques"), ymd(2026, 4, 6));
        assert_eq!(get("Ascension"), ymd(2026, 5, 14));
        assert_eq!(get("Lundi de Pentecôte"), ymd(2026, 5, 25));
        assert_eq!(get("Jour de l’An"), ymd(2026, 1, 1));
        assert!(h.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn school_years() {
        assert_eq!(school_year_start(ymd(2025, 9, 1)), 2025);
        assert_eq!(school_year_start(ymd(2025, 8, 31)), 2024);
        assert_eq!(school_year_start(ymd(2026, 10, 16)), 2026);
        assert_eq!(school_year_label(2025), "2025-2026");
    }

    #[test]
    fn weekdays() {
        assert_eq!(weekday_fr(ymd(2026, 3, 14)), "samedi");
        assert!(is_weekend(ymd(2026, 3, 14)));
        assert!(!is_weekend(ymd(2026, 3, 13)));
    }

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2028));
        assert!(!is_leap_year(2026));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
    }

    #[test]
    fn local_midnight_in_summer_and_winter() {
        assert_eq!(
            local_midnight_utc(ymd(2026, 7, 1), Paris).to_rfc3339(),
            "2026-06-30T22:00:00+00:00"
        );
        assert_eq!(
            local_midnight_utc(ymd(2026, 1, 1), Paris).to_rfc3339(),
            "2025-12-31T23:00:00+00:00"
        );
    }

    #[test]
    fn each_day_is_inclusive() {
        let v: Vec<_> = each_day(ymd(2026, 2, 27), ymd(2026, 3, 1)).collect();
        assert_eq!(v.len(), 3);
        assert_eq!(each_day(ymd(2026, 2, 27), ymd(2026, 2, 26)).count(), 0);
    }
}
