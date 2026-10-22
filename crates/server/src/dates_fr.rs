//! Date formatting (French, and English for the few server-generated English texts).

use chrono::{Datelike, NaiveDate};

const MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

/// `16 octobre 2026` (`1er` for the first of the month).
pub fn long_date(d: NaiveDate) -> String {
    let day = if d.day() == 1 {
        "1er".to_string()
    } else {
        d.day().to_string()
    };
    format!("{day} {} {}", MONTHS[d.month0() as usize], d.year())
}

/// `16 octobre` for labels.
pub fn short_date(d: NaiveDate) -> String {
    let day = if d.day() == 1 {
        "1er".to_string()
    } else {
        d.day().to_string()
    };
    format!("{day} {}", MONTHS[d.month0() as usize])
}

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `16 October` for English labels.
pub fn short_date_en(d: NaiveDate) -> String {
    format!("{} {}", d.day(), MONTHS_EN[d.month0() as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        assert_eq!(long_date(d), "16 octobre 2026");
        assert_eq!(short_date(d), "16 octobre");
        assert_eq!(short_date_en(d), "16 October");
        assert_eq!(
            long_date(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
            "1er février 2026"
        );
    }
}
