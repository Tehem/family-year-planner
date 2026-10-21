//! Thin typed client over the Google Calendar v3 `events` endpoints.

use std::sync::Arc;

use annee_domain::CalendarKind;
use reqwest::{Method, StatusCode};
use serde_json::Value;

use super::auth::{TokenProvider, truncate};
use super::model::{GCalendar, GEvent, GEventList};
use crate::store::StoreError;

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    PreconditionFailed,
    Upstream(String),
}

impl From<ApiError> for StoreError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::NotFound => StoreError::NotFound,
            ApiError::PreconditionFailed => StoreError::Conflict,
            ApiError::Upstream(m) => StoreError::Upstream(m),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => ApiError::NotFound,
            StoreError::Conflict => ApiError::PreconditionFailed,
            StoreError::Upstream(m) => ApiError::Upstream(m),
        }
    }
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    auth: Arc<dyn TokenProvider>,
    reperes_id: String,
    famille_id: String,
}

#[derive(Default, Clone)]
pub struct ListParams {
    pub time_min: Option<String>,
    pub time_max: Option<String>,
    pub private_property: Option<String>,
    /// Return recurring series as one master event instead of expanding them.
    pub masters: bool,
}

impl Client {
    pub fn new(
        http: reqwest::Client,
        base: impl Into<String>,
        auth: Arc<dyn TokenProvider>,
        reperes_id: String,
        famille_id: String,
    ) -> Self {
        Client {
            http,
            base: base.into().trim_end_matches('/').to_string(),
            auth,
            reperes_id,
            famille_id,
        }
    }

    pub fn calendar_id(&self, cal: CalendarKind) -> &str {
        match cal {
            CalendarKind::Reperes => &self.reperes_id,
            CalendarKind::Famille => &self.famille_id,
        }
    }

    fn url(&self, cal: &str, tail: &[&str]) -> Result<reqwest::Url, ApiError> {
        let mut url =
            reqwest::Url::parse(&self.base).map_err(|e| ApiError::Upstream(e.to_string()))?;
        {
            let mut seg = url
                .path_segments_mut()
                .map_err(|_| ApiError::Upstream("bad base url".into()))?;
            seg.pop_if_empty()
                .extend(["calendar", "v3", "calendars", cal]);
            seg.extend(tail);
        }
        Ok(url)
    }

    async fn send(
        &self,
        method: Method,
        url: reqwest::Url,
        query: &[(&str, String)],
        body: Option<&dyn ErasedBody>,
        if_match: Option<&str>,
    ) -> Result<reqwest::Response, ApiError> {
        let token = self.auth.access_token().await?;
        let method_needs_length = method == Method::POST;
        let mut req = self
            .http
            .request(method, url)
            .bearer_auth(token)
            .query(query);
        if let Some(etag) = if_match {
            req = req.header("If-Match", etag);
        }
        if let Some(b) = body {
            req = req.json(&b.as_value());
        } else if method_needs_length {
            // Google's front end answers `411 Length Required` to a body-less POST (`events.move`)
            // unless `Content-Length: 0` is explicit; over HTTP/2 the client omits it otherwise.
            req = req.header(reqwest::header::CONTENT_LENGTH, "0");
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Upstream(format!("request failed: {e}")))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        match status {
            StatusCode::NOT_FOUND | StatusCode::GONE => Err(ApiError::NotFound),
            StatusCode::PRECONDITION_FAILED => Err(ApiError::PreconditionFailed),
            _ => {
                let text = resp.text().await.unwrap_or_default();
                Err(ApiError::Upstream(format!(
                    "Google answered {status}: {}",
                    truncate(&text)
                )))
            }
        }
    }

    pub async fn list(&self, cal: CalendarKind, p: &ListParams) -> Result<Vec<GEvent>, ApiError> {
        let url = self.url(self.calendar_id(cal), &["events"])?;
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let mut q: Vec<(&str, String)> = vec![
                ("singleEvents", (!p.masters).to_string()),
                ("showDeleted", "false".into()),
                ("maxResults", "2500".into()),
            ];
            if let Some(t) = &p.time_min {
                q.push(("timeMin", t.clone()));
            }
            if let Some(t) = &p.time_max {
                q.push(("timeMax", t.clone()));
            }
            if let Some(prop) = &p.private_property {
                q.push(("privateExtendedProperty", prop.clone()));
            }
            if let Some(tok) = &page {
                q.push(("pageToken", tok.clone()));
            }
            let resp = self.send(Method::GET, url.clone(), &q, None, None).await?;
            let list: GEventList = resp
                .json()
                .await
                .map_err(|e| ApiError::Upstream(format!("invalid list: {e}")))?;
            out.extend(list.items);
            match list.next_page_token {
                Some(t) if !t.is_empty() => page = Some(t),
                _ => break,
            }
        }
        Ok(out)
    }

    pub async fn get(&self, cal: CalendarKind, id: &str) -> Result<GEvent, ApiError> {
        let url = self.url(self.calendar_id(cal), &["events", id])?;
        let resp = self.send(Method::GET, url, &[], None, None).await?;
        resp.json()
            .await
            .map_err(|e| ApiError::Upstream(format!("invalid event: {e}")))
    }

    pub async fn insert(&self, cal: CalendarKind, event: &GEvent) -> Result<GEvent, ApiError> {
        let url = self.url(self.calendar_id(cal), &["events"])?;
        let resp = self.send(Method::POST, url, &[], Some(event), None).await?;
        resp.json()
            .await
            .map_err(|e| ApiError::Upstream(format!("invalid event: {e}")))
    }

    pub async fn patch(
        &self,
        cal: CalendarKind,
        id: &str,
        body: &Value,
        etag: Option<&str>,
    ) -> Result<GEvent, ApiError> {
        let url = self.url(self.calendar_id(cal), &["events", id])?;
        let resp = self.send(Method::PATCH, url, &[], Some(body), etag).await?;
        resp.json()
            .await
            .map_err(|e| ApiError::Upstream(format!("invalid event: {e}")))
    }

    pub async fn delete(&self, cal: CalendarKind, id: &str) -> Result<(), ApiError> {
        let url = self.url(self.calendar_id(cal), &["events", id])?;
        self.send(Method::DELETE, url, &[], None, None).await?;
        Ok(())
    }

    /// `events.move`: returns the moved event (now in `to`).
    pub async fn move_to(
        &self,
        from: CalendarKind,
        id: &str,
        to: CalendarKind,
    ) -> Result<GEvent, ApiError> {
        let url = self.url(self.calendar_id(from), &["events", id, "move"])?;
        let q = [("destination", self.calendar_id(to).to_string())];
        let resp = self.send(Method::POST, url, &q, None, None).await?;
        resp.json()
            .await
            .map_err(|e| ApiError::Upstream(format!("invalid event: {e}")))
    }

    /// Calendar metadata (used by `annee check-config`).
    pub async fn calendar(&self, cal: CalendarKind) -> Result<GCalendar, ApiError> {
        let url = self.url(self.calendar_id(cal), &[])?;
        let resp = self.send(Method::GET, url, &[], None, None).await?;
        resp.json()
            .await
            .map_err(|e| ApiError::Upstream(format!("invalid calendar: {e}")))
    }
}

/// Lets `send` take either a typed struct or a JSON value as request body.
pub trait ErasedBody: Sync {
    fn as_value(&self) -> Value;
}

impl ErasedBody for GEvent {
    fn as_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

impl ErasedBody for Value {
    fn as_value(&self) -> Value {
        self.clone()
    }
}
