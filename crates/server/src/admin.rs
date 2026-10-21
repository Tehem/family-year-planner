//! Admin API (`/api/admin/*`): tokens, OAuth grants, imports.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::ApiJson;
use crate::error::ApiError;
use crate::session::Member;
use crate::state::AppState;
use crate::tokens::{Scope, TokenError};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/import", get(import_status).post(import_now))
        .route("/admin/tokens", get(list_tokens).post(create_token))
        .route("/admin/tokens/{id}", delete(revoke_token))
        .route("/admin/oauth-grants", get(list_grants))
        .route("/admin/oauth-grants/{id}", delete(revoke_grant))
}

impl From<TokenError> for ApiError {
    fn from(e: TokenError) -> Self {
        match &e {
            TokenError::Database(detail) => {
                tracing::error!(error = %detail, "token storage error");
                ApiError::internal()
            }
            TokenError::NameTaken => {
                ApiError::new(StatusCode::CONFLICT, "name_taken", e.to_string())
            }
            _ => ApiError::new(StatusCode::BAD_REQUEST, "validation_failed", e.to_string()),
        }
    }
}

async fn list_tokens(State(s): State<AppState>, _m: Member) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s.tokens.list().await?)))
}

#[derive(Deserialize)]
struct NewTokenBody {
    #[serde(default)]
    name: String,
    #[serde(default = "default_scope")]
    scope: String,
    #[serde(default)]
    kiosk: bool,
}

fn default_scope() -> String {
    "read".to_string()
}

async fn create_token(
    State(s): State<AppState>,
    m: Member,
    ApiJson(body): ApiJson<NewTokenBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let scope =
        Scope::parse(&body.scope).ok_or_else(|| ApiError::bad_request("Droits invalides."))?;
    let mut name = body.name.trim().to_string();
    if name.is_empty() && body.kiosk {
        name = format!("Kiosque – {}", crate::dates_fr::short_date(s.today()));
    }
    let (pat, clear) = s.tokens.create(&name, scope, body.kiosk, &m.name).await?;
    let mut out = json!({"id": pat.id, "token": clear});
    if pat.kiosk {
        out["kiosk_url"] = json!(format!("{}/kiosk?token={}", s.config.app.base_url, clear));
    }
    Ok((StatusCode::CREATED, Json(out)))
}

async fn revoke_token(
    State(s): State<AppState>,
    _m: Member,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if s.tokens.revoke(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

async fn import_status(State(s): State<AppState>, _m: Member) -> Result<Json<Value>, ApiError> {
    let status = s.imports.status().await.map_err(|e| {
        tracing::error!(error = %e, "cannot read import runs");
        ApiError::internal()
    })?;
    Ok(Json(
        json!({"running": status.running, "runs": status.runs}),
    ))
}

/// Start an import in the background; if one is already running, report that one.
async fn import_now(State(s): State<AppState>, _m: Member) -> (StatusCode, Json<Value>) {
    s.imports.spawn("manual");
    (StatusCode::ACCEPTED, Json(json!({"running": true})))
}

async fn list_grants(State(s): State<AppState>, _m: Member) -> Result<Json<Value>, ApiError> {
    let grants = s.oauth.list_grants().await.map_err(|e| {
        tracing::error!(error = %e, "cannot list OAuth grants");
        ApiError::internal()
    })?;
    Ok(Json(json!(grants)))
}

async fn revoke_grant(
    State(s): State<AppState>,
    _m: Member,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    match s.oauth.revoke_grant(&id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(ApiError::not_found()),
        Err(e) => {
            tracing::error!(error = %e, "cannot revoke OAuth grant");
            Err(ApiError::internal())
        }
    }
}
