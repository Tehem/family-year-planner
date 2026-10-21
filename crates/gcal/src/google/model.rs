//! Typed subset of the Google Calendar REST API (`events` resource).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GTime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}

impl GTime {
    pub fn all_day(date: impl Into<String>) -> Self {
        GTime {
            date: Some(date.into()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GExtended {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub private: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<GTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<GTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurring_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extended_properties: Option<GExtended>,
}

impl GEvent {
    pub fn private(&self, key: &str) -> Option<&str> {
        self.extended_properties
            .as_ref()
            .and_then(|e| e.private.get(key))
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GEventList {
    #[serde(default)]
    pub items: Vec<GEvent>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GCalendar {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}
