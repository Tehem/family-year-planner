//! Free periods (§5.5), used by the MCP tool `find_free_periods`.

use std::collections::HashSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::dates::each_day;
use crate::types::{Event, EventType};

pub const MAX_PERIODS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum During {
    #[default]
    Any,
    SchoolHolidays,
    OutsideSchoolHolidays,
}

#[derive(Debug, Clone)]
pub struct FreeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub min_days: u32,
    /// Requested members; empty means every family member.
    pub members: Vec<String>,
    pub during: During,
    pub ignore_types: Vec<EventType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub days: u32,
}

/// Maximal runs of consecutive free candidate days of at least `min_days`, sorted by start,
/// capped at [`MAX_PERIODS`].
///
/// `all_members` is the configured family; an empty `query.members` selects all of them.
pub fn find_free_periods(
    events: &[Event],
    all_members: &[String],
    query: &FreeQuery,
) -> Vec<Period> {
    let wanted: Vec<String> = if query.members.is_empty() {
        all_members.to_vec()
    } else {
        query.members.clone()
    };

    let mut busy: HashSet<NaiveDate> = HashSet::new();
    let mut school: HashSet<NaiveDate> = HashSet::new();
    for e in events {
        if !e.overlaps(query.from, query.to) {
            continue;
        }
        let (a, b) = (e.start.max(query.from), e.end.min(query.to));
        if e.kind == EventType::SchoolHoliday {
            school.extend(each_day(a, b));
        }
        if e.kind.makes_busy() && !query.ignore_types.contains(&e.kind) && e.involves_any(&wanted) {
            busy.extend(each_day(a, b));
        }
    }

    let min_days = query.min_days.max(1);
    let mut periods = Vec::new();
    let mut run_start: Option<NaiveDate> = None;
    let mut run_len = 0u32;
    let mut last = query.from;
    let flush = |start: Option<NaiveDate>, len: u32, end: NaiveDate, out: &mut Vec<Period>| {
        if let Some(start) = start
            && len >= min_days
        {
            out.push(Period {
                start,
                end,
                days: len,
            });
        }
    };
    for day in each_day(query.from, query.to) {
        let candidate = match query.during {
            During::Any => true,
            During::SchoolHolidays => school.contains(&day),
            During::OutsideSchoolHolidays => !school.contains(&day),
        };
        let free = candidate && !busy.contains(&day);
        if free {
            if run_start.is_none() {
                run_start = Some(day);
                run_len = 0;
            }
            run_len += 1;
            last = day;
        } else {
            flush(run_start.take(), run_len, last, &mut periods);
            run_len = 0;
        }
    }
    flush(run_start.take(), run_len, last, &mut periods);
    periods.truncate(MAX_PERIODS);
    periods
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::ymd;
    use crate::types::{CalendarKind, EventId, Source};

    fn all() -> Vec<String> {
        ["Alex", "Camille", "Sacha", "Noé"]
            .map(String::from)
            .to_vec()
    }

    fn ev(kind: EventType, start: NaiveDate, end: NaiveDate, members: &[&str]) -> Event {
        Event {
            id: EventId::new(CalendarKind::Famille, format!("{start}{kind}")),
            series_id: None,
            calendar: CalendarKind::Famille,
            kind,
            title: "t".into(),
            start,
            end,
            start_time: None,
            end_time: None,
            members: members.iter().map(|s| s.to_string()).collect(),
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

    fn q(from: NaiveDate, to: NaiveDate, min_days: u32) -> FreeQuery {
        FreeQuery {
            from,
            to,
            min_days,
            members: vec![],
            during: During::Any,
            ignore_types: vec![],
        }
    }

    #[test]
    fn empty_calendar_is_one_big_period() {
        let p = find_free_periods(&[], &all(), &q(ymd(2026, 7, 1), ymd(2026, 7, 31), 1));
        assert_eq!(
            p,
            vec![Period {
                start: ymd(2026, 7, 1),
                end: ymd(2026, 7, 31),
                days: 31
            }]
        );
    }

    #[test]
    fn busy_days_split_periods_with_boundaries_inclusive() {
        let events = [ev(EventType::Trip, ymd(2026, 7, 10), ymd(2026, 7, 12), &[])];
        let p = find_free_periods(&events, &all(), &q(ymd(2026, 7, 1), ymd(2026, 7, 31), 1));
        assert_eq!(
            p,
            vec![
                Period {
                    start: ymd(2026, 7, 1),
                    end: ymd(2026, 7, 9),
                    days: 9
                },
                Period {
                    start: ymd(2026, 7, 13),
                    end: ymd(2026, 7, 31),
                    days: 19
                },
            ]
        );
    }

    #[test]
    fn min_days_filters_short_runs() {
        let events = [ev(EventType::Trip, ymd(2026, 7, 4), ymd(2026, 7, 4), &[])];
        let p = find_free_periods(&events, &all(), &q(ymd(2026, 7, 1), ymd(2026, 7, 10), 4));
        assert_eq!(
            p,
            vec![Period {
                start: ymd(2026, 7, 5),
                end: ymd(2026, 7, 10),
                days: 6
            }]
        );
        let p = find_free_periods(&events, &all(), &q(ymd(2026, 7, 1), ymd(2026, 7, 10), 7));
        assert!(p.is_empty());
    }

    #[test]
    fn only_requested_members_matter() {
        let events = [ev(
            EventType::FamilyHome,
            ymd(2026, 7, 10),
            ymd(2026, 7, 12),
            &["Sacha"],
        )];
        let mut query = q(ymd(2026, 7, 8), ymd(2026, 7, 14), 1);
        query.members = vec!["Alex".into()];
        assert_eq!(
            find_free_periods(&events, &all(), &query).len(),
            1,
            "Alex is free"
        );
        query.members = vec!["Sacha".into(), "Alex".into()];
        assert_eq!(
            find_free_periods(&events, &all(), &query).len(),
            2,
            "Sacha is busy"
        );
        query.members = vec![];
        assert_eq!(
            find_free_periods(&events, &all(), &query).len(),
            2,
            "default is everybody"
        );
    }

    #[test]
    fn events_without_members_involve_everyone() {
        let events = [ev(
            EventType::Celebration,
            ymd(2026, 7, 10),
            ymd(2026, 7, 10),
            &[],
        )];
        let mut query = q(ymd(2026, 7, 9), ymd(2026, 7, 11), 1);
        query.members = vec!["Noé".into()];
        assert_eq!(find_free_periods(&events, &all(), &query).len(), 2);
    }

    #[test]
    fn birthdays_daycare_and_public_holidays_never_make_a_day_busy() {
        let events = [
            ev(EventType::Birthday, ymd(2026, 7, 10), ymd(2026, 7, 10), &[]),
            ev(
                EventType::DaycareClosure,
                ymd(2026, 7, 11),
                ymd(2026, 7, 11),
                &[],
            ),
            ev(
                EventType::PublicHoliday,
                ymd(2026, 7, 14),
                ymd(2026, 7, 14),
                &[],
            ),
        ];
        let p = find_free_periods(&events, &all(), &q(ymd(2026, 7, 1), ymd(2026, 7, 31), 1));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].days, 31);
    }

    #[test]
    fn unclassified_is_busy_unless_ignored() {
        let events = [ev(
            EventType::Unclassified,
            ymd(2026, 7, 10),
            ymd(2026, 7, 10),
            &[],
        )];
        let mut query = q(ymd(2026, 7, 9), ymd(2026, 7, 11), 1);
        assert_eq!(find_free_periods(&events, &all(), &query).len(), 2);
        query.ignore_types = vec![EventType::Unclassified];
        assert_eq!(find_free_periods(&events, &all(), &query).len(), 1);
    }

    #[test]
    fn during_school_holidays_and_outside() {
        let events = [
            ev(
                EventType::SchoolHoliday,
                ymd(2026, 7, 4),
                ymd(2026, 8, 31),
                &[],
            ),
            ev(EventType::Trip, ymd(2026, 7, 10), ymd(2026, 7, 12), &[]),
        ];
        let mut query = q(ymd(2026, 6, 28), ymd(2026, 7, 14), 1);
        query.during = During::SchoolHolidays;
        let p = find_free_periods(&events, &all(), &query);
        assert_eq!(
            p,
            vec![
                Period {
                    start: ymd(2026, 7, 4),
                    end: ymd(2026, 7, 9),
                    days: 6
                },
                Period {
                    start: ymd(2026, 7, 13),
                    end: ymd(2026, 7, 14),
                    days: 2
                },
            ]
        );
        query.during = During::OutsideSchoolHolidays;
        let p = find_free_periods(&events, &all(), &query);
        assert_eq!(
            p,
            vec![Period {
                start: ymd(2026, 6, 28),
                end: ymd(2026, 7, 3),
                days: 6
            }]
        );
    }

    #[test]
    fn events_outside_the_window_are_clipped() {
        let events = [ev(EventType::Trip, ymd(2026, 6, 25), ymd(2026, 7, 3), &[])];
        let p = find_free_periods(&events, &all(), &q(ymd(2026, 7, 1), ymd(2026, 7, 10), 1));
        assert_eq!(
            p,
            vec![Period {
                start: ymd(2026, 7, 4),
                end: ymd(2026, 7, 10),
                days: 7
            }]
        );
    }

    #[test]
    fn fully_busy_window_has_no_period() {
        let events = [ev(EventType::Trip, ymd(2026, 6, 25), ymd(2026, 7, 31), &[])];
        assert!(
            find_free_periods(&events, &all(), &q(ymd(2026, 7, 1), ymd(2026, 7, 10), 1)).is_empty()
        );
    }

    #[test]
    fn at_most_fifty_periods() {
        // Alternate busy/free days over 200 days → 100 single-day gaps.
        let events: Vec<Event> = (0..100)
            .map(|i| {
                let d = ymd(2026, 1, 2) + chrono::Duration::days(2 * i);
                ev(EventType::Trip, d, d, &[])
            })
            .collect();
        let p = find_free_periods(&events, &all(), &q(ymd(2026, 1, 1), ymd(2026, 12, 31), 1));
        assert_eq!(p.len(), MAX_PERIODS);
        assert!(p.windows(2).all(|w| w[0].start < w[1].start));
    }
}
