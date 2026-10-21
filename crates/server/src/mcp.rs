//! MCP server (§13): Streamable HTTP endpoint `/mcp`, authenticated with personal access tokens
//! or OAuth access tokens, exposing the family calendar as tools.

use std::sync::Arc;

use annee_domain::free::{During, FreeQuery, find_free_periods};
use annee_domain::{Event, EventInput, EventPatch, EventType, TypeInfo, ValidationError};
use annee_gcal::StoreError;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, HeaderValue, ORIGIN, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use chrono::{Datelike, NaiveDate};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, JsonObject,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{Map, Value, json};

use crate::error::{ApiError, ServiceError};
use crate::ratelimit::too_many;
use crate::state::AppState;
use crate::tokens::{PAT_PREFIX, Scope};

const MAX_LIST_SPAN_DAYS: i64 = 400;
const MAX_LIST_EVENTS: usize = 500;

const INSTRUCTIONS: &str = "\
Family Year Planner: a shared family year planner backed by two Google calendars.\n\
- 'Repères' holds context days: school holidays (type school_holiday, 'Vacances scolaires'), daycare or leisure-centre closures (daycare_closure, 'Fermeture de la crèche ou du centre de loisirs'), birthdays (birthday, 'Anniversaire') and public holidays (public_holiday, 'Jour férié'). School and public holidays are imported automatically and are read-only.\n\
- 'Famille' holds family events: family_gathering ('Réunion de famille'), family_home ('Séjour en maison de famille'), trip ('Voyage ou vacances'), celebration ('Fête ou cérémonie'). Events of unknown category are 'unclassified' ('Non classé').\n\
Conventions: dates are ISO YYYY-MM-DD in the Europe/Paris time zone and end dates are INCLUSIVE (a trip from 2026-07-25 to 2026-08-02 covers both days). Members are the first names returned by get_context; an empty member list means the whole family. Write titles in French. Birthdays last one day and repeat every year; editing or deleting one occurrence affects the whole series.\n\
Call get_context first to learn today's date, the members and the type codes.";

/// Who is calling `/mcp`.
#[derive(Debug, Clone)]
pub struct McpAuth {
    /// PAT name or OAuth client name: recorded as `fy_created_by` / `fy_updated_by`.
    pub name: String,
    pub scope: Scope,
    /// Rate-limit key and log identity.
    pub key: String,
}

fn unauthorized(state: &AppState) -> Response {
    let mut res = (
        StatusCode::UNAUTHORIZED,
        axum::Json(
            json!({"error": {"code": "unauthorized", "message": "Authentification requise."}}),
        ),
    )
        .into_response();
    let challenge = format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        state.config.app.base_url
    );
    if let Ok(v) = HeaderValue::from_str(&challenge) {
        res.headers_mut().insert(WWW_AUTHENTICATE, v);
    }
    res
}

/// The credential in `Authorization`: `Bearer <token>`, or the bare token (some agent platforms
/// pass the configured value as is).
pub fn credential(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    let token = match raw.split_once(' ') {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("bearer") => rest.trim(),
        _ => raw,
    };
    (!token.is_empty()).then(|| token.to_string())
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Option<McpAuth> {
    let token = credential(headers)?;
    if token.starts_with(PAT_PREFIX) {
        let pat = state.tokens.authenticate(&token).await.ok()??;
        if pat.kiosk {
            return None; // kiosk links only open the read-only tablet view
        }
        return Some(McpAuth {
            name: pat.name,
            scope: pat.scope,
            key: format!("pat:{}", pat.id),
        });
    }
    crate::oauth::authenticate_access_token(state, &token).await
}

async fn mcp_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    // Origin, when present, must be ours (DNS-rebinding / cross-site protection).
    if let Some(origin) = req.headers().get(ORIGIN)
        && origin.to_str().ok() != Some(state.config.origin().as_str())
    {
        return ApiError::forbidden("Origine non autorisée.").into_response();
    }
    let Some(auth) = authenticate(&state, req.headers()).await else {
        return unauthorized(&state);
    };
    if !state.limits.mcp.check(&auth.key) {
        return too_many();
    }
    req.extensions_mut().insert(auth);
    next.run(req).await
}

/// `/mcp` router (auth + rmcp service).
pub fn router(state: AppState) -> Router<AppState> {
    let host = state
        .config
        .app
        .base_url
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(rest).to_string())
        .unwrap_or_default();
    let mut config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_origins([state.config.origin()]);
    config = if state.is_demo() {
        config.disable_allowed_hosts()
    } else {
        // `host` may carry a port; accept it with and without.
        let bare = host.split(':').next().unwrap_or(&host).to_string();
        config.with_allowed_hosts([host.clone(), bare])
    };
    let handler_state = state.clone();
    let service: StreamableHttpService<McpServer, LocalSessionManager> = StreamableHttpService::new(
        move || {
            Ok(McpServer {
                state: handler_state.clone(),
            })
        },
        Default::default(),
        config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(state, mcp_auth))
}

#[derive(Clone)]
pub struct McpServer {
    state: AppState,
}

type ToolResult = Result<Value, ToolError>;

/// A tool-level failure: English code, French message.
#[derive(Debug)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
}

impl ToolError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        ToolError {
            code,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        ToolError::new("invalid_params", message)
    }
}

impl From<ServiceError> for ToolError {
    fn from(e: ServiceError) -> Self {
        match e {
            ServiceError::Validation(v) => v.into(),
            ServiceError::Store(s) => s.into(),
            ServiceError::BadId => ToolError::new("not_found", "Événement introuvable."),
        }
    }
}

impl From<ValidationError> for ToolError {
    fn from(e: ValidationError) -> Self {
        ToolError::new(e.code(), e.to_string())
    }
}

impl From<StoreError> for ToolError {
    fn from(e: StoreError) -> Self {
        let code = match &e {
            StoreError::NotFound => "not_found",
            StoreError::Conflict => "conflict",
            StoreError::Upstream(detail) => {
                tracing::error!(detail = %detail, "Google Calendar error (MCP)");
                "upstream_error"
            }
        };
        ToolError::new(code, e.to_string())
    }
}

fn schema(v: Value) -> Arc<JsonObject> {
    Arc::new(v.as_object().cloned().unwrap_or_default())
}

fn date_prop(desc: &str) -> Value {
    json!({"type": "string", "format": "date", "pattern": "^\\d{4}-\\d{2}-\\d{2}$", "description": desc})
}

fn type_codes(only_writable: bool) -> Vec<&'static str> {
    EventType::ALL
        .into_iter()
        .filter(|t| !only_writable || t.writable())
        .map(EventType::code)
        .collect()
}

fn members_prop(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}

/// The tool catalogue.
pub fn tools() -> Vec<Tool> {
    let ro = || ToolAnnotations::new().read_only(true);
    let event_fields = |writable_only_types: bool| {
        json!({
            "type": {"type": "string", "enum": type_codes(writable_only_types), "description": "Event category. Writable types: birthday (Anniversaire), celebration (Fête ou cérémonie), family_gathering (Réunion de famille), family_home (Séjour en maison de famille), trip (Voyage ou vacances), daycare_closure (Fermeture de la crèche ou du centre de loisirs)."},
            "title": {"type": "string", "maxLength": 120, "description": "Title in French, 1 to 120 characters."},
            "start": date_prop("First day, YYYY-MM-DD."),
            "end": date_prop("Last day, YYYY-MM-DD, INCLUSIVE (>= start, at most 366 days). Defaults to start. Birthdays last exactly one day."),
            "members": members_prop("First names of the people concerned (from get_context). Empty or omitted = the whole family."),
            "location": {"type": ["string", "null"], "maxLength": 200, "description": "Optional place."},
            "notes": {"type": ["string", "null"], "maxLength": 2000, "description": "Optional free-text notes."},
            "birth_year": {"type": ["integer", "null"], "minimum": 1900, "description": "Birthdays only: year of birth, used to display the age (e.g. 'Anniversaire de Sacha (8 ans)')."}
        })
    };
    let create_props = event_fields(true);
    let mut update_props = event_fields(true);
    if let Some(o) = update_props.as_object_mut() {
        o.insert("id".into(), json!({"type": "string", "description": "Event id as returned by list_events/get_day, e.g. 'f:abc123'. For an occurrence of a yearly birthday the whole series is edited."}));
    }
    vec![
        Tool::new(
            "get_context",
            "Get today's date (Europe/Paris), the time zone, the family members (use these exact first names), the event types with their codes and French labels, and the school zone. Call this first.",
            schema(json!({"type": "object", "properties": {}, "additionalProperties": false})),
        )
        .annotate(ro().with_title_hint("Contexte")),
        Tool::new(
            "list_events",
            "List calendar events overlapping a date range (inclusive end dates, at most 400 days), sorted by start date, at most 500. Includes family events, birthdays (with age when the birth year is known), daycare closures, school holidays and public holidays. Optional filters: types (type codes), member (a first name; events for the whole family always match) and query (case-insensitive substring of title, location or notes).",
            schema(json!({
                "type": "object",
                "properties": {
                    "from": date_prop("First day of the range, YYYY-MM-DD."),
                    "to": date_prop("Last day of the range, YYYY-MM-DD, inclusive."),
                    "types": {"type": "array", "items": {"type": "string", "enum": type_codes(false)}, "description": "Only these event types."},
                    "member": {"type": "string", "description": "Only events involving this member (first name)."},
                    "query": {"type": "string", "description": "Case-insensitive text search in title, location and notes."}
                },
                "required": ["from", "to"],
                "additionalProperties": false
            })),
        )
        .annotate(ro()),
        Tool::new(
            "get_day",
            "Describe one day: weekday, weekend flag, public holiday name, school holiday name, whether the daycare is closed, the birthdays and all other events of that day.",
            schema(json!({
                "type": "object",
                "properties": {"date": date_prop("The day, YYYY-MM-DD.")},
                "required": ["date"],
                "additionalProperties": false
            })),
        )
        .annotate(ro()),
        Tool::new(
            "find_free_periods",
            "Find periods when the requested family members have no family events (gatherings, family-home stays, trips, celebrations). Birthdays, daycare closures and public holidays do not make a day busy. Use `during` to restrict to school holidays or exclude them. Returns maximal runs of consecutive free days of at least min_days, sorted by start, at most 50.",
            schema(json!({
                "type": "object",
                "properties": {
                    "from": date_prop("First day of the search window, YYYY-MM-DD."),
                    "to": date_prop("Last day of the search window, YYYY-MM-DD, inclusive (window of at most 400 days)."),
                    "min_days": {"type": "integer", "minimum": 1, "default": 1, "description": "Minimum length of a period in days."},
                    "members": members_prop("Members who must all be free; default: the whole family."),
                    "during": {"type": "string", "enum": ["any", "school_holidays", "outside_school_holidays"], "default": "any", "description": "Restrict to school-holiday days, or to days outside school holidays."},
                    "ignore_types": {"type": "array", "items": {"type": "string", "enum": type_codes(false)}, "description": "Event types that should not make a day busy (e.g. unclassified)."}
                },
                "required": ["from", "to"],
                "additionalProperties": false
            })),
        )
        .annotate(ro()),
        Tool::new(
            "create_event",
            "Create an event in the family calendar. The calendar is chosen from the type. Dates are inclusive ISO dates; `end` defaults to `start`. Titles should be in French. Requires a read-write credential. Returns the created event.",
            schema(json!({
                "type": "object",
                "properties": create_props,
                "required": ["type", "title", "start"],
                "additionalProperties": false
            })),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(false).idempotent(false)),
        Tool::new(
            "update_event",
            "Update an event by id: send only the fields to change. Pass null for location, notes or birth_year to clear them. Changing the type may move the event to the other calendar. Imported events (school and public holidays) cannot be modified. Editing an occurrence of a yearly birthday edits the whole series. Requires a read-write credential. Returns the updated event.",
            schema(json!({
                "type": "object",
                "properties": update_props,
                "required": ["id"],
                "additionalProperties": false
            })),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(false).idempotent(true)),
        Tool::new(
            "delete_event",
            "Delete an event by id. Deleting an occurrence of a yearly birthday deletes the whole series (scope 'series'). Imported events cannot be deleted. Requires a read-write credential.",
            schema(json!({
                "type": "object",
                "properties": {"id": {"type": "string", "description": "Event id as returned by list_events/get_day."}},
                "required": ["id"],
                "additionalProperties": false
            })),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(true).idempotent(true)),
    ]
}

trait AnnotationExt {
    fn with_title_hint(self, title: &str) -> Self;
}

impl AnnotationExt for ToolAnnotations {
    fn with_title_hint(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }
}

fn parse_date(args: &Map<String, Value>, key: &str) -> Result<NaiveDate, ToolError> {
    let s = args.get(key).and_then(Value::as_str).ok_or_else(|| {
        ToolError::invalid(format!("Le paramètre « {key} » est requis (AAAA-MM-JJ)."))
    })?;
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| {
        ToolError::invalid(format!(
            "Date invalide pour « {key} » : {s} (format AAAA-MM-JJ)."
        ))
    })
}

fn parse_types(args: &Map<String, Value>, key: &str) -> Result<Vec<EventType>, ToolError> {
    let Some(v) = args.get(key).filter(|v| !v.is_null()) else {
        return Ok(vec![]);
    };
    let arr = v
        .as_array()
        .ok_or_else(|| ToolError::invalid(format!("« {key} » doit être une liste.")))?;
    arr.iter()
        .map(|c| {
            c.as_str()
                .and_then(EventType::from_code)
                .ok_or_else(|| ToolError::invalid(format!("Type inconnu dans « {key} » : {c}.")))
        })
        .collect()
}

fn check_members(state: &AppState, names: &[String]) -> Result<(), ToolError> {
    for n in names {
        if !state.config.family.members.contains(n) {
            return Err(ValidationError::UnknownMember(n.clone()).into());
        }
    }
    Ok(())
}

fn parse_members(
    state: &AppState,
    args: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ToolError> {
    let Some(v) = args.get(key).filter(|v| !v.is_null()) else {
        return Ok(vec![]);
    };
    let names: Vec<String> = serde_json::from_value(v.clone())
        .map_err(|_| ToolError::invalid(format!("« {key} » doit être une liste de prénoms.")))?;
    check_members(state, &names)?;
    Ok(names)
}

fn check_span(from: NaiveDate, to: NaiveDate) -> Result<(), ToolError> {
    if to < from {
        return Err(ValidationError::EndBeforeStart.into());
    }
    if (to - from).num_days() + 1 > MAX_LIST_SPAN_DAYS {
        return Err(ToolError::invalid(format!(
            "La période ne peut pas dépasser {MAX_LIST_SPAN_DAYS} jours."
        )));
    }
    Ok(())
}

fn ev(e: &Event) -> Value {
    serde_json::to_value(e).unwrap_or(Value::Null)
}

impl McpServer {
    async fn get_context(&self) -> ToolResult {
        let s = &self.state;
        let types: Vec<Value> = TypeInfo::all()
            .into_iter()
            .map(|t| json!({"code": t.code, "label": t.label, "calendar": t.calendar, "writable": t.writable}))
            .collect();
        Ok(json!({
            "today": s.today(),
            "timezone": s.config.app.timezone,
            "members": s.config.family.members,
            "types": types,
            "school_zone": s.config.imports.school_zone,
        }))
    }

    async fn list_events(&self, args: &Map<String, Value>) -> ToolResult {
        let (from, to) = (parse_date(args, "from")?, parse_date(args, "to")?);
        check_span(from, to)?;
        let types = parse_types(args, "types")?;
        let member = args
            .get("member")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(m) = &member {
            check_members(&self.state, std::slice::from_ref(m))?;
        }
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .map(str::to_lowercase)
            .filter(|q| !q.trim().is_empty());
        let events = self.state.store.list_range(from, to).await?;
        let matches = |e: &Event| {
            (types.is_empty() || types.contains(&e.kind))
                && member
                    .as_ref()
                    .is_none_or(|m| e.involves_any(std::slice::from_ref(m)))
                && query.as_ref().is_none_or(|q| {
                    [Some(&e.title), e.location.as_ref(), e.notes.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|f| f.to_lowercase().contains(q))
                })
        };
        let out: Vec<Value> = events
            .iter()
            .filter(|e| matches(e))
            .take(MAX_LIST_EVENTS)
            .map(ev)
            .collect();
        Ok(json!({"events": out}))
    }

    async fn get_day(&self, args: &Map<String, Value>) -> ToolResult {
        let date = parse_date(args, "date")?;
        let events = self.state.events_for_year(date.year()).await?;
        let day: Vec<&Event> = events.iter().filter(|e| e.covers(date)).collect();
        let first_title = |k: EventType| day.iter().find(|e| e.kind == k).map(|e| e.title.clone());
        let weekday = [
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
        ][annee_domain::dates::weekday_index(date) as usize];
        let mut out = json!({
            "date": date,
            "weekday": weekday,
            "is_weekend": annee_domain::dates::is_weekend(date),
            "daycare_closed": day.iter().any(|e| e.kind == EventType::DaycareClosure),
            "birthdays": day.iter().filter(|e| e.kind == EventType::Birthday).map(|e| ev(e)).collect::<Vec<_>>(),
            "events": day
                .iter()
                .filter(|e| !matches!(e.kind, EventType::Birthday | EventType::PublicHoliday | EventType::SchoolHoliday))
                .map(|e| ev(e))
                .collect::<Vec<_>>(),
        });
        if let Some(t) = first_title(EventType::PublicHoliday) {
            out["public_holiday"] = json!(t);
        }
        if let Some(t) = first_title(EventType::SchoolHoliday) {
            out["school_holiday"] = json!(t);
        }
        Ok(out)
    }

    async fn find_free_periods(&self, args: &Map<String, Value>) -> ToolResult {
        let (from, to) = (parse_date(args, "from")?, parse_date(args, "to")?);
        check_span(from, to)?;
        let min_days = match args.get("min_days").filter(|v| !v.is_null()) {
            None => 1,
            Some(v) => v
                .as_u64()
                .filter(|n| (1..=u32::MAX as u64).contains(n))
                .ok_or_else(|| {
                    ToolError::invalid("« min_days » doit être un entier supérieur ou égal à 1.")
                })? as u32,
        };
        let during = match args.get("during").filter(|v| !v.is_null()) {
            None => During::Any,
            Some(v) => serde_json::from_value(v.clone()).map_err(|_| {
                ToolError::invalid(
                    "« during » doit valoir any, school_holidays ou outside_school_holidays.",
                )
            })?,
        };
        let members = parse_members(&self.state, args, "members")?;
        let ignore_types = parse_types(args, "ignore_types")?;
        let events = self.state.store.list_range(from, to).await?;
        let periods = find_free_periods(
            &events,
            &self.state.config.family.members,
            &FreeQuery {
                from,
                to,
                min_days,
                members,
                during,
                ignore_types,
            },
        );
        Ok(json!({"periods": periods}))
    }

    async fn create_event(&self, args: Map<String, Value>, auth: &McpAuth) -> ToolResult {
        let input: EventInput = serde_json::from_value(Value::Object(args))
            .map_err(|_| ToolError::invalid("Paramètres invalides pour create_event."))?;
        let event = self.state.create_event(&input, &auth.name).await?;
        Ok(ev(&event))
    }

    async fn update_event(&self, mut args: Map<String, Value>, auth: &McpAuth) -> ToolResult {
        let id = args
            .remove("id")
            .and_then(|v| v.as_str().map(str::to_string))
            .ok_or_else(|| ToolError::invalid("Le paramètre « id » est requis."))?;
        let patch: EventPatch = serde_json::from_value(Value::Object(args))
            .map_err(|_| ToolError::invalid("Paramètres invalides pour update_event."))?;
        let event = self.state.update_event(&id, &patch, &auth.name).await?;
        Ok(ev(&event))
    }

    async fn delete_event(&self, args: &Map<String, Value>) -> ToolResult {
        let id = args
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid("Le paramètre « id » est requis."))?;
        let (id, scope) = self.state.delete_event(id).await?;
        Ok(json!({"deleted": true, "id": id, "scope": scope.as_str()}))
    }
}

fn tool_error(e: ToolError) -> CallToolResponse {
    CallToolResult::structured_error(json!({"code": e.code, "message": e.message})).into()
}

fn is_write_tool(name: &str) -> bool {
    matches!(name, "create_event" | "update_event" | "delete_event")
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "family-year-planner",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|t| t.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let auth = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|p| p.extensions.get::<McpAuth>())
            .cloned()
            .ok_or_else(|| McpError::internal_error("missing authentication context", None))?;
        let name = request.name.to_string();
        let args = request.arguments.unwrap_or_default();
        tracing::info!(tool = %name, agent = %auth.name, "mcp tool call");

        if is_write_tool(&name) && !auth.scope.can_write() {
            return Ok(tool_error(ToolError::new(
                "insufficient_scope",
                "Ce jeton ne permet que la lecture.",
            )));
        }
        let result = match name.as_str() {
            "get_context" => self.get_context().await,
            "list_events" => self.list_events(&args).await,
            "get_day" => self.get_day(&args).await,
            "find_free_periods" => self.find_free_periods(&args).await,
            "create_event" => self.create_event(args, &auth).await,
            "update_event" => self.update_event(args, &auth).await,
            "delete_event" => self.delete_event(&args).await,
            other => {
                return Err(McpError::invalid_params(
                    format!("Unknown tool: {other}"),
                    None,
                ));
            }
        };
        Ok(match result {
            Ok(v) => CallToolResult::structured(v).into(),
            Err(e) => tool_error(e),
        })
    }
}
