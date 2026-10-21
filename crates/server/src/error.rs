//! API errors: `{ "error": { "code", "message" } }` with a French message.

use annee_domain::ValidationError;
use annee_gcal::StoreError;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized() -> Self {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Connectez-vous pour continuer.",
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn read_only_session() -> Self {
        ApiError::forbidden("Cette session est en lecture seule.")
    }

    pub fn not_found() -> Self {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", "Introuvable.")
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    pub fn internal() -> Self {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Une erreur est survenue.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"error": {"code": self.code, "message": self.message}})),
        )
            .into_response()
    }
}

impl From<ValidationError> for ApiError {
    fn from(e: ValidationError) -> Self {
        let status = if e == ValidationError::ReadOnly {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::BAD_REQUEST
        };
        ApiError::new(status, e.code(), e.to_string())
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match &e {
            StoreError::NotFound => {
                ApiError::new(StatusCode::NOT_FOUND, "not_found", e.to_string())
            }
            StoreError::Conflict => ApiError::new(StatusCode::CONFLICT, "conflict", e.to_string()),
            StoreError::Upstream(detail) => {
                tracing::error!(detail = %detail, "Google Calendar error");
                ApiError::new(StatusCode::BAD_GATEWAY, "upstream_error", e.to_string())
            }
        }
    }
}

/// Failure of an operation shared by the web API and MCP.
#[derive(Debug, Clone)]
pub enum ServiceError {
    Validation(ValidationError),
    Store(StoreError),
    BadId,
}

impl From<ValidationError> for ServiceError {
    fn from(e: ValidationError) -> Self {
        ServiceError::Validation(e)
    }
}

impl From<StoreError> for ServiceError {
    fn from(e: StoreError) -> Self {
        ServiceError::Store(e)
    }
}

impl From<ServiceError> for ApiError {
    fn from(e: ServiceError) -> Self {
        match e {
            ServiceError::Validation(v) => v.into(),
            ServiceError::Store(s) => s.into(),
            ServiceError::BadId => {
                ApiError::new(StatusCode::NOT_FOUND, "not_found", "Événement introuvable.")
            }
        }
    }
}
