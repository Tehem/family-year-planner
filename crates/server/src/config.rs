//! Configuration: TOML file (§3) and environment variables (§15.1).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use chrono_tz::Tz;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Prod,
    Demo,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Prod => "prod",
            Mode::Demo => "demo",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSection {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub allowed_emails: Vec<String>,
    #[serde(default)]
    pub user_names: BTreeMap<String, String>,
    /// `fr` or `en` (§19.2).
    #[serde(default = "default_language")]
    pub default_language: String,
}

impl Default for AppSection {
    fn default() -> Self {
        AppSection {
            base_url: default_base_url(),
            timezone: default_timezone(),
            allowed_emails: Vec::new(),
            user_names: BTreeMap::new(),
            default_language: default_language(),
        }
    }
}

fn default_language() -> String {
    "fr".to_string()
}
fn default_base_url() -> String {
    "http://localhost:8080".to_string()
}
fn default_timezone() -> String {
    "Europe/Paris".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilySection {
    #[serde(default = "default_members")]
    pub members: Vec<String>,
}

impl Default for FamilySection {
    fn default() -> Self {
        FamilySection {
            members: default_members(),
        }
    }
}

fn default_members() -> Vec<String> {
    ["Alex", "Camille", "Sacha", "Noé"]
        .map(String::from)
        .to_vec()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarsSection {
    #[serde(default)]
    pub reperes_id: String,
    #[serde(default)]
    pub famille_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportsSection {
    #[serde(default = "default_zone")]
    pub school_zone: String,
    #[serde(default = "default_location")]
    pub school_location: String,
    #[serde(default = "default_public_holidays")]
    pub public_holidays: String,
    #[serde(default = "default_schedule")]
    pub schedule: String,
    #[serde(default = "default_school_years_ahead")]
    pub school_years_ahead: u32,
    #[serde(default = "default_ph_years_ahead")]
    pub public_holiday_years_ahead: u32,
}

impl Default for ImportsSection {
    fn default() -> Self {
        ImportsSection {
            school_zone: default_zone(),
            school_location: default_location(),
            public_holidays: default_public_holidays(),
            schedule: default_schedule(),
            school_years_ahead: default_school_years_ahead(),
            public_holiday_years_ahead: default_ph_years_ahead(),
        }
    }
}

fn default_zone() -> String {
    "Zone B".to_string()
}
fn default_location() -> String {
    "Rennes".to_string()
}
fn default_public_holidays() -> String {
    "metropole".to_string()
}
fn default_schedule() -> String {
    "0 0 4 1 * *".to_string()
}
fn default_school_years_ahead() -> u32 {
    1
}
fn default_ph_years_ahead() -> u32 {
    2
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KioskSection {
    #[serde(default = "default_refresh")]
    pub refresh_minutes: u32,
    #[serde(default = "default_view")]
    pub default_view: String,
}

impl Default for KioskSection {
    fn default() -> Self {
        KioskSection {
            refresh_minutes: default_refresh(),
            default_view: default_view(),
        }
    }
}

fn default_refresh() -> u32 {
    5
}
fn default_view() -> String {
    "week".to_string()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    #[serde(default)]
    pub app: AppSection,
    #[serde(default)]
    pub family: FamilySection,
    #[serde(default)]
    pub calendars: CalendarsSection,
    #[serde(default)]
    pub imports: ImportsSection,
    #[serde(default)]
    pub kiosk: KioskSection,
}

impl AppConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("invalid configuration: {e}"))
    }

    /// Read the file at `path`. A missing file is an error unless `optional`.
    pub fn load(path: &Path, optional: bool) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && optional => {
                Ok(AppConfig::default())
            }
            Err(e) => Err(format!("cannot read configuration {}: {e}", path.display())),
        }
    }

    pub fn tz(&self) -> Tz {
        self.app
            .timezone
            .parse()
            .unwrap_or(chrono_tz::Europe::Paris)
    }

    /// `https://host` (no path): the origin of `base_url`.
    pub fn origin(&self) -> String {
        match self.app.base_url.split_once("://") {
            Some((scheme, rest)) => {
                format!("{scheme}://{}", rest.split('/').next().unwrap_or(rest))
            }
            None => self.app.base_url.clone(),
        }
    }

    /// Language used when a request carries no language cookie (§19.2).
    pub fn default_lang(&self) -> annee_domain::Lang {
        annee_domain::Lang::parse(&self.app.default_language).unwrap_or_default()
    }

    pub fn secure_cookies(&self) -> bool {
        self.app.base_url.starts_with("https://")
    }

    pub fn mcp_url(&self) -> String {
        format!("{}/mcp", self.app.base_url)
    }

    /// Name shown/stored for a logged-in email: `[app.user_names]` or the email's local part.
    pub fn user_name(&self, email: &str) -> String {
        let lower = email.to_lowercase();
        self.app
            .user_names
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| lower.split('@').next().unwrap_or(&lower).to_string())
    }

    pub fn is_allowed(&self, email: &str) -> bool {
        let lower = email.to_lowercase();
        self.app
            .allowed_emails
            .iter()
            .any(|a| a.to_lowercase() == lower)
    }

    /// Structural checks shared by all modes; `prod` adds the required values.
    pub fn validate(&self, mode: Mode) -> Result<(), String> {
        let mut problems = Vec::new();
        let base = &self.app.base_url;
        if !(base.starts_with("https://") || base.starts_with("http://")) || base.ends_with('/') {
            problems.push(
                "app.base_url must start with http:// or https:// and have no trailing slash"
                    .to_string(),
            );
        }
        if annee_domain::Lang::parse(&self.app.default_language).is_none() {
            problems.push("app.default_language must be \"fr\" or \"en\"".into());
        }
        if self.app.timezone.parse::<Tz>().is_err() {
            problems.push(format!(
                "app.timezone: unknown time zone {:?}",
                self.app.timezone
            ));
        }
        let members = &self.family.members;
        if members.is_empty() {
            problems.push("family.members must not be empty".into());
        }
        for (i, m) in members.iter().enumerate() {
            if m.trim().is_empty() || m.contains(',') || m != m.trim() {
                problems.push(format!(
                    "family.members: invalid name {m:?} (no commas, no surrounding spaces)"
                ));
            }
            if members[..i].contains(m) {
                problems.push(format!("family.members: duplicate name {m:?}"));
            }
        }
        if self.imports.public_holidays != "metropole" {
            problems.push("imports.public_holidays: only \"metropole\" is supported".into());
        }
        if !["week", "month"].contains(&self.kiosk.default_view.as_str()) {
            problems.push("kiosk.default_view must be \"week\" or \"month\"".into());
        }
        if self.kiosk.refresh_minutes == 0 {
            problems.push("kiosk.refresh_minutes must be at least 1".into());
        }
        if mode == Mode::Prod {
            if self.app.allowed_emails.is_empty() {
                problems.push("app.allowed_emails must list at least one email".into());
            }
            if self.calendars.reperes_id.trim().is_empty() {
                problems.push("calendars.reperes_id is required".into());
            }
            if self.calendars.famille_id.trim().is_empty() {
                problems.push("calendars.famille_id is required".into());
            }
            if self.calendars.reperes_id == self.calendars.famille_id
                && !self.calendars.reperes_id.is_empty()
            {
                problems.push("calendars.reperes_id and calendars.famille_id must differ".into());
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n"))
        }
    }
}

/// Environment variables (§15.1).
#[derive(Debug, Clone)]
pub struct Env {
    pub mode: Mode,
    pub config_path: PathBuf,
    pub bind: String,
    pub database_url: String,
    pub service_account_file: Option<PathBuf>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub session_key: Option<String>,
    pub oauth_signing_key: Option<String>,
    pub fake_today: Option<NaiveDate>,
}

impl Env {
    /// Read from a variable lookup (the process environment in production).
    pub fn from_lookup(
        get: impl Fn(&str) -> Option<String>,
        force_demo: bool,
    ) -> Result<Env, String> {
        let non_empty = |k: &str| {
            get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let mode = if force_demo {
            Mode::Demo
        } else {
            match non_empty("APP_MODE").as_deref() {
                None | Some("prod") => Mode::Prod,
                Some("demo") => Mode::Demo,
                Some(other) => {
                    return Err(format!(
                        "APP_MODE must be \"prod\" or \"demo\", got {other:?}"
                    ));
                }
            }
        };
        let fake_today = match (mode, non_empty("APP_FAKE_TODAY")) {
            (Mode::Demo, Some(v)) => Some(
                NaiveDate::parse_from_str(&v, "%Y-%m-%d")
                    .map_err(|_| format!("APP_FAKE_TODAY must be YYYY-MM-DD, got {v:?}"))?,
            ),
            _ => None,
        };
        Ok(Env {
            mode,
            config_path: non_empty("APP_CONFIG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/config/config.toml".into()),
            bind: non_empty("BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            database_url: non_empty("DATABASE_URL")
                .unwrap_or_else(|| "sqlite:///data/annee.db".into()),
            service_account_file: non_empty("GOOGLE_SERVICE_ACCOUNT_FILE").map(PathBuf::from),
            google_client_id: non_empty("GOOGLE_OAUTH_CLIENT_ID"),
            google_client_secret: non_empty("GOOGLE_OAUTH_CLIENT_SECRET"),
            session_key: non_empty("SESSION_KEY"),
            oauth_signing_key: non_empty("OAUTH_SIGNING_KEY"),
            fake_today,
        })
    }

    pub fn from_process(force_demo: bool) -> Result<Env, String> {
        Env::from_lookup(|k| std::env::var(k).ok(), force_demo)
    }

    /// In prod, name every missing required secret.
    pub fn require_prod_secrets(&self, names: &[&str]) -> Result<(), String> {
        if self.mode != Mode::Prod {
            return Ok(());
        }
        let missing: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| match *n {
                "GOOGLE_SERVICE_ACCOUNT_FILE" => self.service_account_file.is_none(),
                "GOOGLE_OAUTH_CLIENT_ID" => self.google_client_id.is_none(),
                "GOOGLE_OAUTH_CLIENT_SECRET" => self.google_client_secret.is_none(),
                "SESSION_KEY" => self.session_key.is_none(),
                "OAUTH_SIGNING_KEY" => self.oauth_signing_key.is_none(),
                _ => false,
            })
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "missing required environment variable(s): {}",
                missing.join(", ")
            ))
        }
    }
}

pub const SERVER_SECRETS: [&str; 5] = [
    "GOOGLE_SERVICE_ACCOUNT_FILE",
    "GOOGLE_OAUTH_CLIENT_ID",
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "SESSION_KEY",
    "OAUTH_SIGNING_KEY",
];

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
[app]
base_url = "https://famille.example.org"
timezone = "Europe/Paris"
allowed_emails = ["Alex@Example.com", "camille@example.com"]
[app.user_names]
"alex@example.com" = "Alex"
[family]
members = ["Alex", "Camille", "Sacha", "Noé"]
[calendars]
reperes_id = "a@group.calendar.google.com"
famille_id = "b@group.calendar.google.com"
[imports]
school_zone = "Zone B"
school_location = "Rennes"
[kiosk]
refresh_minutes = 5
default_view = "week"
"#;

    #[test]
    fn example_config_is_valid() {
        let c = AppConfig::parse(EXAMPLE).unwrap();
        c.validate(Mode::Prod).unwrap();
        assert_eq!(c.imports.schedule, "0 0 4 1 * *", "defaults apply");
        assert!(c.is_allowed("alex@example.com"));
        assert_eq!(c.user_name("ALEX@example.com"), "Alex");
        assert_eq!(c.user_name("camille@example.com"), "camille");
        assert_eq!(c.origin(), "https://famille.example.org");
        assert_eq!(c.mcp_url(), "https://famille.example.org/mcp");
        assert!(c.secure_cookies());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let e = AppConfig::parse("[app]\nbase_urll = \"x\"").unwrap_err();
        assert!(e.contains("base_urll"), "{e}");
    }

    #[test]
    fn prod_requires_calendars_and_emails() {
        let c = AppConfig::default();
        let e = c.validate(Mode::Prod).unwrap_err();
        assert!(
            e.contains("allowed_emails") && e.contains("reperes_id") && e.contains("famille_id"),
            "{e}"
        );
        c.validate(Mode::Demo).unwrap();
    }

    #[test]
    fn structural_problems_are_all_reported() {
        let mut c = AppConfig::parse(EXAMPLE).unwrap();
        c.app.base_url = "famille.example.org/".into();
        c.app.timezone = "Mars/Olympus".into();
        c.family.members = vec!["A,B".into(), "C".into(), "C".into()];
        c.kiosk.default_view = "year".into();
        c.imports.public_holidays = "alsace".into();
        let e = c.validate(Mode::Prod).unwrap_err();
        for needle in [
            "base_url",
            "timezone",
            "invalid name",
            "duplicate",
            "default_view",
            "public_holidays",
        ] {
            assert!(e.contains(needle), "{needle} missing in {e}");
        }
    }

    #[test]
    fn env_names_missing_secrets() {
        let env = Env::from_lookup(|_| None, false).unwrap();
        assert_eq!(env.mode, Mode::Prod);
        let e = env.require_prod_secrets(&SERVER_SECRETS).unwrap_err();
        for n in SERVER_SECRETS {
            assert!(e.contains(n), "{n}");
        }
        let env =
            Env::from_lookup(|k| (k == "SESSION_KEY").then(|| "x".to_string()), false).unwrap();
        assert!(!env.require_prod_secrets(&["SESSION_KEY"]).is_err());
    }

    #[test]
    fn demo_ignores_missing_secrets_and_accepts_fake_today() {
        let env = Env::from_lookup(
            |k| match k {
                "APP_MODE" => Some("demo".into()),
                "APP_FAKE_TODAY" => Some("2026-10-16".into()),
                _ => None,
            },
            false,
        )
        .unwrap();
        assert_eq!(env.mode, Mode::Demo);
        env.require_prod_secrets(&SERVER_SECRETS).unwrap();
        assert_eq!(env.fake_today, NaiveDate::from_ymd_opt(2026, 10, 16));
    }

    #[test]
    fn fake_today_is_ignored_in_prod() {
        let env = Env::from_lookup(
            |k| (k == "APP_FAKE_TODAY").then(|| "2026-10-16".to_string()),
            false,
        )
        .unwrap();
        assert_eq!(env.fake_today, None);
    }

    #[test]
    fn bad_mode_is_an_error() {
        assert!(
            Env::from_lookup(|k| (k == "APP_MODE").then(|| "staging".to_string()), false).is_err()
        );
    }
}
