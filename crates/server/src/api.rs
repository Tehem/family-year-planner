//! JSON API used by the web app (§7).

use annee_domain::{EventInput, EventPatch, TypeInfo};
use axum::extract::{FromRequest, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::ApiError;
use crate::security::csrf_guard_api;
use crate::session::{Member, Principal};
use crate::state::AppState;

/// JSON body extractor whose rejections are French API errors.
pub struct ApiJson<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(ApiJson(v)),
            Err(_) => Err(ApiError::bad_request("Requête invalide.")),
        }
    }
}

pub fn router(state: AppState) -> Router<AppState> {
    let r = Router::new()
        .route("/me", get(me))
        .route("/config", get(config))
        .route("/events", get(list_events).post(create_event))
        .route(
            "/events/{id}",
            axum::routing::patch(update_event).delete(delete_event),
        );
    let r = if state.is_demo() {
        r.route("/demo/reset", post(demo_reset))
    } else {
        r
    };
    r.merge(crate::admin::router())
        .layer(middleware::from_fn_with_state(state, csrf_guard_api))
}

async fn me(State(s): State<AppState>, p: Principal) -> Json<Value> {
    let (email, name, kiosk) = match &p {
        Principal::Member { email, name } => (email.clone(), name.clone(), false),
        Principal::Kiosk { .. } => (String::new(), "Kiosque".to_string(), true),
    };
    Json(json!({"email": email, "name": name, "kiosk": kiosk, "demo": s.is_demo()}))
}

async fn config(State(s): State<AppState>, _p: Principal) -> Json<Value> {
    Json(json!({
        "members": s.config.family.members,
        "types": TypeInfo::all(),
        "timezone": s.config.app.timezone,
        "school_zone": s.config.imports.school_zone,
        "today": s.today(),
        "base_url": s.config.app.base_url,
        "mcp_url": s.config.mcp_url(),
        "kiosk": {
            "refresh_minutes": s.config.kiosk.refresh_minutes,
            "default_view": s.config.kiosk.default_view,
        },
    }))
}

#[derive(Deserialize)]
struct YearQuery {
    year: Option<i32>,
}

async fn list_events(
    State(s): State<AppState>,
    _p: Principal,
    Query(q): Query<YearQuery>,
) -> Result<Json<Value>, ApiError> {
    let year = q.year.unwrap_or_else(|| chrono::Datelike::year(&s.today()));
    if !(1900..=2200).contains(&year) {
        return Err(ApiError::bad_request("Année invalide."));
    }
    let events = s.events_for_year(year).await?;
    Ok(Json(json!({"year": year, "events": events})))
}

async fn create_event(
    State(s): State<AppState>,
    m: Member,
    ApiJson(input): ApiJson<EventInput>,
) -> Result<(StatusCode, Json<annee_domain::Event>), ApiError> {
    let event = s.create_event(&input, &m.name).await?;
    Ok((StatusCode::CREATED, Json(event)))
}

async fn update_event(
    State(s): State<AppState>,
    m: Member,
    Path(id): Path<String>,
    ApiJson(patch): ApiJson<EventPatch>,
) -> Result<Json<annee_domain::Event>, ApiError> {
    Ok(Json(s.update_event(&id, &patch, &m.name).await?))
}

async fn delete_event(
    State(s): State<AppState>,
    _m: Member,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    s.delete_event(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn demo_reset(State(s): State<AppState>, _m: Member) -> StatusCode {
    if let Some(demo) = &s.demo {
        demo.reset();
    }
    StatusCode::NO_CONTENT
}
