//! School holidays from data.education.gouv.fr (§6.1).

use std::time::Duration;

use annee_domain::{EventType, Source};
use annee_gcal::ImportedEvent;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate};
use chrono_tz::Tz;
use serde::Deserialize;

pub const DEFAULT_BASE_URL: &str = "https://data.education.gouv.fr";
const DATASET_PATH: &str = "/api/explore/v2.1/catalog/datasets/fr-en-calendrier-scolaire/records";
const PAGE_SIZE: usize = 100;

/// One record of the `fr-en-calendrier-scolaire` dataset (only the fields we use).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EduRecord {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub population: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub zones: Option<String>,
    #[serde(default)]
    pub annee_scolaire: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Page {
    #[serde(default)]
    total_count: usize,
    #[serde(default)]
    results: Vec<EduRecord>,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct EduError(pub String);

/// Where school-year records come from.
#[async_trait]
pub trait EducationSource: Send + Sync {
    /// Records of a school year (`2025-2026`) for the configured location. Empty when the year
    /// is not published yet.
    async fn fetch(&self, school_year: &str, location: &str) -> Result<Vec<EduRecord>, EduError>;
}

/// The live dataset, with retries (1 s, 4 s, 16 s by default).
pub struct HttpEducation {
    http: reqwest::Client,
    base_url: String,
    backoff: Vec<Duration>,
}

impl HttpEducation {
    pub fn new(http: reqwest::Client) -> Self {
        HttpEducation {
            http,
            base_url: DEFAULT_BASE_URL.to_string(),
            backoff: vec![
                Duration::from_secs(1),
                Duration::from_secs(4),
                Duration::from_secs(16),
            ],
        }
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into().trim_end_matches('/').to_string();
        self
    }

    /// Delays between the attempts (`backoff.len() + 1` attempts in total are NOT made: the spec
    /// asks for 3 attempts, so the first delays are used between them).
    pub fn with_backoff(mut self, backoff: Vec<Duration>) -> Self {
        self.backoff = backoff;
        self
    }

    async fn page(
        &self,
        school_year: &str,
        location: &str,
        offset: usize,
    ) -> Result<Page, EduError> {
        let where_clause = format!(
            "location=\"{}\" and annee_scolaire=\"{}\"",
            escape(location),
            escape(school_year)
        );
        let resp = self
            .http
            .get(format!("{}{DATASET_PATH}", self.base_url))
            .query(&[
                ("where", where_clause),
                ("order_by", "start_date".to_string()),
                ("limit", PAGE_SIZE.to_string()),
                ("offset", offset.to_string()),
            ])
            .send()
            .await
            .map_err(|e| EduError(format!("data.education.gouv.fr is unreachable: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(EduError(format!(
                "data.education.gouv.fr answered {status}"
            )));
        }
        resp.json().await.map_err(|e| {
            EduError(format!(
                "unexpected response from data.education.gouv.fr: {e}"
            ))
        })
    }

    async fn fetch_once(
        &self,
        school_year: &str,
        location: &str,
    ) -> Result<Vec<EduRecord>, EduError> {
        let mut all = Vec::new();
        loop {
            let page = self.page(school_year, location, all.len()).await?;
            let got = page.results.len();
            all.extend(page.results);
            if got == 0 || all.len() >= page.total_count {
                return Ok(all);
            }
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[async_trait]
impl EducationSource for HttpEducation {
    async fn fetch(&self, school_year: &str, location: &str) -> Result<Vec<EduRecord>, EduError> {
        // Three attempts with exponential backoff between them.
        let attempts = 3;
        let mut last = EduError("no attempt".into());
        for attempt in 0..attempts {
            match self.fetch_once(school_year, location).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    tracing::warn!(attempt = attempt + 1, error = %e, "education dataset request failed");
                    last = e;
                    if attempt + 1 < attempts
                        && let Some(delay) = self.backoff.get(attempt)
                    {
                        tokio::time::sleep(*delay).await;
                    }
                }
            }
        }
        Err(last)
    }
}

/// Recorded responses (demo mode and tests): 2024-2025, 2025-2026 and 2026-2027 for Rennes.
pub struct FixtureEducation;

#[async_trait]
impl EducationSource for FixtureEducation {
    async fn fetch(&self, school_year: &str, location: &str) -> Result<Vec<EduRecord>, EduError> {
        let json = match school_year {
            "2024-2025" => include_str!("../../tests/fixtures/education_2024-2025.json"),
            "2025-2026" => include_str!("../../tests/fixtures/education_2025-2026.json"),
            "2026-2027" => include_str!("../../tests/fixtures/education_2026-2027.json"),
            _ => return Ok(Vec::new()),
        };
        parse_page(json).map(|v| {
            v.into_iter()
                .filter(|r| r.location.as_deref() == Some(location))
                .collect()
        })
    }
}

pub fn parse_page(json: &str) -> Result<Vec<EduRecord>, EduError> {
    serde_json::from_str::<Page>(json)
        .map(|p| p.results)
        .map_err(|e| EduError(format!("invalid dataset page: {e}")))
}

/// Lowercase ASCII slug: `Vacances d'Été` → `vacances-d-ete`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(fold) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn fold(c: char) -> Vec<char> {
    match c {
        'à' | 'â' | 'ä' | 'À' | 'Â' | 'Ä' => vec!['a'],
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => vec!['e'],
        'î' | 'ï' | 'Î' | 'Ï' => vec!['i'],
        'ô' | 'ö' | 'Ô' | 'Ö' => vec!['o'],
        'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => vec!['u'],
        'ç' | 'Ç' => vec!['c'],
        'œ' | 'Œ' => vec!['o', 'e'],
        'æ' | 'Æ' => vec!['a', 'e'],
        c => vec![c],
    }
}

const PROPER_NOUNS: [&str; 8] = [
    "noël",
    "toussaint",
    "ascension",
    "pâques",
    "pentecôte",
    "carnaval",
    "armistice",
    "mai",
];

/// French sentence case: `Vacances d'Été` → `Vacances d’été`, `Pont de l'Ascension` →
/// `Pont de l’Ascension`, `Vacances de la Toussaint` unchanged.
pub fn normalize_title(description: &str) -> String {
    let text = description.trim().replace('\'', "’");
    let mut out = String::new();
    let mut word = String::new();
    let mut first = true;
    let flush = |word: &mut String, out: &mut String, first: &mut bool| {
        if word.is_empty() {
            return;
        }
        let lower = word.to_lowercase();
        let w = if *first || PROPER_NOUNS.contains(&lower.as_str()) {
            capitalize(&lower)
        } else {
            lower
        };
        out.push_str(&w);
        word.clear();
        *first = false;
    };
    for c in text.chars() {
        if c.is_alphabetic() || c == '-' {
            word.push(c);
        } else {
            flush(&mut word, &mut out, &mut first);
            out.push(c);
        }
    }
    flush(&mut word, &mut out, &mut first);
    out
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// What was skipped while converting records (for logs).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Skipped {
    pub other_zone: usize,
    pub teachers: usize,
    pub malformed: usize,
}

fn local_date(s: &str, tz: Tz) -> Option<NaiveDate> {
    Some(
        DateTime::parse_from_rfc3339(s)
            .ok()?
            .with_timezone(&tz)
            .date_naive(),
    )
}

/// Convert the records of one school year into events (§6.1).
///
/// The dataset gives instants at local midnight: `start_date` is the first day without school and
/// `end_date` the day classes resume, so the holiday is `[local(start), local(end) − 1 day]`.
/// Summer holidays without `end_date` end on 31 August.
pub fn school_events(
    school_year: &str,
    records: &[EduRecord],
    zone: &str,
    tz: Tz,
) -> (Vec<ImportedEvent>, Skipped) {
    let mut skipped = Skipped::default();
    let mut out: Vec<ImportedEvent> = Vec::new();
    for r in records {
        if r.annee_scolaire
            .as_deref()
            .is_some_and(|y| y != school_year)
        {
            continue;
        }
        if r.zones.as_deref() != Some(zone) {
            skipped.other_zone += 1;
            continue;
        }
        match r.population.as_deref().map(str::trim) {
            None | Some("-") | Some("") | Some("Élèves") => {}
            Some(_) => {
                skipped.teachers += 1;
                continue;
            }
        }
        let (Some(description), Some(start)) = (
            r.description.as_deref(),
            r.start_date.as_deref().and_then(|s| local_date(s, tz)),
        ) else {
            skipped.malformed += 1;
            continue;
        };
        let end = match r.end_date.as_deref().and_then(|s| local_date(s, tz)) {
            Some(resume) => (resume - chrono::Duration::days(1)).max(start),
            None => summer_fallback_end(school_year, start),
        };
        let source_id = format!("edu:{school_year}:{}:{start}", slug(description));
        if out.iter().any(|e| e.source_id == source_id) {
            continue; // same holiday listed for several populations
        }
        out.push(ImportedEvent {
            kind: EventType::SchoolHoliday,
            title: normalize_title(description),
            start,
            end,
            source: Source::ImportEducation,
            source_id,
        });
    }
    out.sort_by_key(|e| (e.start, e.source_id.clone()));
    (out, skipped)
}

/// A holiday without `end_date` ends on 31 August of the year it starts in (or after).
fn summer_fallback_end(school_year: &str, start: NaiveDate) -> NaiveDate {
    let end_year = school_year
        .split('-')
        .nth(1)
        .and_then(|y| y.parse::<i32>().ok())
        .unwrap_or(start.year());
    NaiveDate::from_ymd_opt(end_year, 8, 31)
        .filter(|d| *d >= start)
        .unwrap_or(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use annee_domain::dates::ymd;
    use chrono_tz::Europe::Paris;

    fn fixture(year: &str) -> Vec<EduRecord> {
        let json = match year {
            "2024-2025" => include_str!("../../tests/fixtures/education_2024-2025.json"),
            "2025-2026" => include_str!("../../tests/fixtures/education_2025-2026.json"),
            _ => include_str!("../../tests/fixtures/education_2026-2027.json"),
        };
        parse_page(json).unwrap()
    }

    #[test]
    fn titles_are_normalized_to_french_sentence_case() {
        for (raw, expected) in [
            ("Vacances d'Été", "Vacances d’été"),
            ("Vacances d'Hiver", "Vacances d’hiver"),
            ("Vacances de Printemps", "Vacances de printemps"),
            ("Vacances de Noël", "Vacances de Noël"),
            ("Vacances de la Toussaint", "Vacances de la Toussaint"),
            ("Pont de l'Ascension", "Pont de l’Ascension"),
            ("  VACANCES DE NOËL ", "Vacances de Noël"),
        ] {
            assert_eq!(normalize_title(raw), expected, "{raw}");
        }
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Vacances d'Été"), "vacances-d-ete");
        assert_eq!(slug("Pont de l'Ascension"), "pont-de-l-ascension");
        assert_eq!(slug("Vacances de Noël"), "vacances-de-noel");
    }

    /// Acceptance case of §6.1.
    #[test]
    fn toussaint_2025_2026_zone_b_is_18_october_to_2_november() {
        let (events, skipped) = school_events("2025-2026", &fixture("2025-2026"), "Zone B", Paris);
        let toussaint = events
            .iter()
            .find(|e| e.title == "Vacances de la Toussaint")
            .unwrap();
        assert_eq!(
            (toussaint.start, toussaint.end),
            (ymd(2025, 10, 18), ymd(2025, 11, 2))
        );
        assert_eq!(
            toussaint.source_id,
            "edu:2025-2026:vacances-de-la-toussaint:2025-10-18"
        );
        assert_eq!(
            skipped.teachers, 1,
            "the Enseignants summer record is dropped"
        );
        assert_eq!(events.len(), 6);
    }

    #[test]
    fn full_school_year_2025_2026() {
        let (events, _) = school_events("2025-2026", &fixture("2025-2026"), "Zone B", Paris);
        let got: Vec<_> = events
            .iter()
            .map(|e| (e.title.as_str(), e.start.to_string(), e.end.to_string()))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "Vacances de la Toussaint",
                    "2025-10-18".into(),
                    "2025-11-02".into()
                ),
                ("Vacances de Noël", "2025-12-20".into(), "2026-01-04".into()),
                ("Vacances d’hiver", "2026-02-14".into(), "2026-03-01".into()),
                (
                    "Vacances de printemps",
                    "2026-04-11".into(),
                    "2026-04-26".into()
                ),
                (
                    "Pont de l’Ascension",
                    "2026-05-14".into(),
                    "2026-05-17".into()
                ),
                ("Vacances d’été", "2026-07-04".into(), "2026-08-31".into()),
            ]
        );
    }

    #[test]
    fn school_year_2024_2025_uses_winter_time_offsets_correctly() {
        let (events, _) = school_events("2024-2025", &fixture("2024-2025"), "Zone B", Paris);
        let noel = events
            .iter()
            .find(|e| e.title == "Vacances de Noël")
            .unwrap();
        assert_eq!((noel.start, noel.end), (ymd(2024, 12, 21), ymd(2025, 1, 5)));
        let toussaint = events
            .iter()
            .find(|e| e.title == "Vacances de la Toussaint")
            .unwrap();
        assert_eq!(
            (toussaint.start, toussaint.end),
            (ymd(2024, 10, 19), ymd(2024, 11, 3))
        );
    }

    #[test]
    fn a_zero_length_record_becomes_a_single_day() {
        // The dataset has `start_date == end_date` for the 2027 Ascension bridge.
        let (events, _) = school_events("2026-2027", &fixture("2026-2027"), "Zone B", Paris);
        let pont = events
            .iter()
            .find(|e| e.title == "Pont de l’Ascension")
            .unwrap();
        assert_eq!((pont.start, pont.end), (ymd(2027, 5, 7), ymd(2027, 5, 7)));
        let ete = events.iter().find(|e| e.title == "Vacances d’été").unwrap();
        assert_eq!(
            ete.end,
            ymd(2027, 9, 1),
            "the Élèves record is kept as published"
        );
    }

    #[test]
    fn other_zones_are_dropped() {
        let (events, skipped) = school_events("2025-2026", &fixture("2025-2026"), "Zone C", Paris);
        assert!(events.is_empty());
        assert_eq!(skipped.other_zone, 7);
    }

    #[test]
    fn missing_end_date_summer_ends_on_31_august() {
        let rec = EduRecord {
            description: Some("Vacances d'Été".into()),
            population: Some("Élèves".into()),
            start_date: Some("2026-07-03T22:00:00+00:00".into()),
            end_date: None,
            location: Some("Rennes".into()),
            zones: Some("Zone B".into()),
            annee_scolaire: Some("2025-2026".into()),
        };
        let (events, _) = school_events("2025-2026", &[rec], "Zone B", Paris);
        assert_eq!(
            (events[0].start, events[0].end),
            (ymd(2026, 7, 4), ymd(2026, 8, 31))
        );
    }

    #[test]
    fn malformed_records_are_skipped() {
        let rec = EduRecord {
            description: Some("Vacances".into()),
            population: None,
            start_date: Some("pas une date".into()),
            end_date: None,
            location: None,
            zones: Some("Zone B".into()),
            annee_scolaire: None,
        };
        let (events, skipped) = school_events("2025-2026", &[rec], "Zone B", Paris);
        assert!(events.is_empty());
        assert_eq!(skipped.malformed, 1);
    }

    #[test]
    fn duplicate_populations_yield_one_event() {
        let mk = |pop: &str| EduRecord {
            description: Some("Vacances d'Été".into()),
            population: Some(pop.into()),
            start_date: Some("2026-07-03T22:00:00+00:00".into()),
            end_date: Some("2026-08-31T22:00:00+00:00".into()),
            location: None,
            zones: Some("Zone B".into()),
            annee_scolaire: None,
        };
        let (events, _) = school_events("2025-2026", &[mk("-"), mk("Élèves")], "Zone B", Paris);
        assert_eq!(events.len(), 1);
    }
}
