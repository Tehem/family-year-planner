//! Rendering helpers shared in spirit with the front-end (§5.6): per-day tints, bar lanes.
//!
//! The browser has its own TypeScript implementation; keeping these rules in Rust, with tests,
//! gives the server (e.g. MCP `get_day`) the same semantics.

use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::dates::is_weekend;
use crate::types::{Event, EventType};

/// Maximum number of lanes of bars in a month row / week column.
pub const LANES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
    None,
    School,
    Daycare,
    /// School holiday and daycare closure on the same day.
    Both,
    Birthday,
}

/// Background tint for the given flags (§5.6). A birthday always wins the background; see
/// [`birthday_band`] for the thin band showing the other tint.
pub fn tint(school: bool, daycare: bool, birthday: bool) -> Tint {
    if birthday {
        Tint::Birthday
    } else {
        match (school, daycare) {
            (true, true) => Tint::Both,
            (true, false) => Tint::School,
            (false, true) => Tint::Daycare,
            (false, false) => Tint::None,
        }
    }
}

/// On a birthday that is also a school or daycare day, the other tint appears as a thin band at
/// the bottom of the cell.
pub fn birthday_band(school: bool, daycare: bool, birthday: bool) -> Option<Tint> {
    if !birthday {
        return None;
    }
    match (school, daycare) {
        (true, true) => Some(Tint::Both),
        (true, false) => Some(Tint::School),
        (false, true) => Some(Tint::Daycare),
        (false, false) => None,
    }
}

/// What a given day looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayView {
    pub date: NaiveDate,
    pub tint: Tint,
    pub band: Option<Tint>,
    /// Weekend or public holiday: the weekend shade applies.
    pub shaded: bool,
    /// Public holidays color the day number red.
    pub public_holiday: Option<String>,
    pub school_holiday: Option<String>,
    pub daycare_closed: bool,
    pub birthday: bool,
}

/// Compute the [`DayView`] of `date` from the events of its year.
pub fn day_view(events: &[Event], date: NaiveDate) -> DayView {
    let mut school = None;
    let mut ph = None;
    let (mut daycare, mut birthday) = (false, false);
    for e in events.iter().filter(|e| e.covers(date)) {
        match e.kind {
            EventType::SchoolHoliday => school = school.or_else(|| Some(e.title.clone())),
            EventType::PublicHoliday => ph = ph.or_else(|| Some(e.title.clone())),
            EventType::DaycareClosure => daycare = true,
            EventType::Birthday => birthday = true,
            _ => {}
        }
    }
    DayView {
        date,
        tint: tint(school.is_some(), daycare, birthday),
        band: birthday_band(school.is_some(), daycare, birthday),
        shaded: is_weekend(date) || ph.is_some(),
        public_holiday: ph,
        school_holiday: school,
        daycare_closed: daycare,
        birthday,
    }
}

/// A bar segment on one track (a month row, or a week column in the Weeks view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub track: i64,
    /// First and last position along the track (inclusive).
    pub p0: i64,
    pub p1: i64,
}

/// Lane of each segment (same order as the input). Segments with `lane >= LANES` overflow and
/// are replaced by a grey dot marker on their days.
pub fn assign_lanes(segments: &[Segment]) -> Vec<usize> {
    let mut by_track: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, s) in segments.iter().enumerate() {
        by_track.entry(s.track).or_default().push(i);
    }
    let mut lanes = vec![0; segments.len()];
    for idxs in by_track.values_mut() {
        // Earlier start first; for equal starts the longer segment first.
        idxs.sort_by_key(|&i| {
            (
                segments[i].p0,
                std::cmp::Reverse(segments[i].p1 - segments[i].p0),
            )
        });
        let mut ends: Vec<i64> = Vec::new();
        for &i in idxs.iter() {
            let s = segments[i];
            let lane = match ends.iter().position(|&end| end < s.p0) {
                Some(l) => l,
                None => {
                    ends.push(0);
                    ends.len() - 1
                }
            };
            ends[lane] = s.p1;
            lanes[i] = lane;
        }
    }
    lanes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::ymd;
    use crate::types::{CalendarKind, EventId, Source};

    fn ev(kind: EventType, title: &str, start: NaiveDate, end: NaiveDate) -> Event {
        Event {
            id: EventId::new(CalendarKind::Reperes, title),
            series_id: None,
            calendar: CalendarKind::Reperes,
            kind,
            title: title.into(),
            start,
            end,
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
    fn tints() {
        assert_eq!(tint(false, false, false), Tint::None);
        assert_eq!(tint(true, false, false), Tint::School);
        assert_eq!(tint(false, true, false), Tint::Daycare);
        assert_eq!(tint(true, true, false), Tint::Both);
        assert_eq!(tint(true, true, true), Tint::Birthday);
        assert_eq!(tint(false, false, true), Tint::Birthday);
    }

    #[test]
    fn birthday_bands() {
        assert_eq!(birthday_band(true, false, true), Some(Tint::School));
        assert_eq!(birthday_band(false, true, true), Some(Tint::Daycare));
        assert_eq!(birthday_band(true, true, true), Some(Tint::Both));
        assert_eq!(birthday_band(false, false, true), None);
        assert_eq!(birthday_band(true, true, false), None);
    }

    #[test]
    fn day_view_combines_events() {
        let events = [
            ev(
                EventType::SchoolHoliday,
                "Vacances d’été",
                ymd(2026, 7, 4),
                ymd(2026, 8, 31),
            ),
            ev(
                EventType::DaycareClosure,
                "Crèche fermée",
                ymd(2026, 8, 3),
                ymd(2026, 8, 24),
            ),
            ev(
                EventType::PublicHoliday,
                "Assomption",
                ymd(2026, 8, 15),
                ymd(2026, 8, 15),
            ),
        ];
        let d = day_view(&events, ymd(2026, 8, 4));
        assert_eq!(d.tint, Tint::Both);
        assert!(d.daycare_closed && !d.shaded && d.public_holiday.is_none());
        let d = day_view(&events, ymd(2026, 8, 15)); // Saturday and a public holiday
        assert!(d.shaded);
        assert_eq!(d.public_holiday.as_deref(), Some("Assomption"));
        let d = day_view(&events, ymd(2026, 7, 5)); // Sunday
        assert!(d.shaded);
        assert_eq!(d.tint, Tint::School);
        assert_eq!(day_view(&events, ymd(2026, 9, 2)).tint, Tint::None);
    }

    #[test]
    fn public_holiday_on_a_weekday_is_shaded() {
        let events = [ev(
            EventType::PublicHoliday,
            "Fête nationale",
            ymd(2026, 7, 14),
            ymd(2026, 7, 14),
        )];
        let d = day_view(&events, ymd(2026, 7, 14)); // Tuesday
        assert!(d.shaded);
    }

    #[test]
    fn birthday_on_school_day_keeps_green_with_band() {
        let events = [
            ev(
                EventType::SchoolHoliday,
                "Toussaint",
                ymd(2026, 10, 17),
                ymd(2026, 11, 1),
            ),
            ev(
                EventType::Birthday,
                "Anniversaire",
                ymd(2026, 10, 20),
                ymd(2026, 10, 20),
            ),
        ];
        let d = day_view(&events, ymd(2026, 10, 20));
        assert_eq!(d.tint, Tint::Birthday);
        assert_eq!(d.band, Some(Tint::School));
    }

    fn seg(track: i64, p0: i64, p1: i64) -> Segment {
        Segment { track, p0, p1 }
    }

    #[test]
    fn lanes_reuse_free_lanes() {
        let lanes = assign_lanes(&[seg(1, 0, 3), seg(1, 5, 7), seg(1, 2, 6)]);
        assert_eq!(lanes, vec![0, 0, 1]);
    }

    #[test]
    fn longer_segment_wins_the_first_lane_on_equal_start() {
        let lanes = assign_lanes(&[seg(1, 0, 1), seg(1, 0, 5)]);
        assert_eq!(lanes, vec![1, 0]);
    }

    #[test]
    fn third_overlapping_segment_overflows() {
        let lanes = assign_lanes(&[seg(1, 0, 6), seg(1, 1, 5), seg(1, 2, 4)]);
        assert_eq!(lanes, vec![0, 1, 2]);
        assert!(lanes[2] >= LANES);
    }

    #[test]
    fn tracks_are_independent() {
        let lanes = assign_lanes(&[seg(1, 0, 6), seg(2, 0, 6)]);
        assert_eq!(lanes, vec![0, 0]);
    }

    #[test]
    fn touching_segments_do_not_share_a_lane() {
        // p1 == next p0: they overlap on that day.
        let lanes = assign_lanes(&[seg(1, 0, 3), seg(1, 3, 5)]);
        assert_eq!(lanes, vec![0, 1]);
    }
}
