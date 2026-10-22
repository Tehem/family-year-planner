//! API errors: `{ "error": { "code", "message" } }` with a message in the request language.

use annee_domain::ValidationError;
use annee_gcal::StoreError;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::i18n::{current, tr};
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
            tr("Connectez-vous pour continuer.", "Sign in to continue."),
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn read_only_session() -> Self {
        ApiError::forbidden(tr(
            "Cette session est en lecture seule.",
            "This session is read-only.",
        ))
    }

    pub fn not_found() -> Self {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            tr("Introuvable.", "Not found."),
        )
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    pub fn internal() -> Self {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            tr("Une erreur est survenue.", "Something went wrong."),
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
        ApiError::new(status, e.code(), e.message(current()))
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        let message = store_message(&e);
        match &e {
            StoreError::NotFound => ApiError::new(StatusCode::NOT_FOUND, "not_found", message),
            StoreError::Conflict => ApiError::new(StatusCode::CONFLICT, "conflict", message),
            StoreError::Upstream(detail) => {
                tracing::error!(detail = %detail, "Google Calendar error");
                ApiError::new(StatusCode::BAD_GATEWAY, "upstream_error", message)
            }
        }
    }
}

/// The message of a store failure in the request language.
pub fn store_message(e: &StoreError) -> String {
    match e {
        StoreError::NotFound => tr("Événement introuvable.", "Event not found."),
        StoreError::Conflict => tr(
            "Cet événement a été modifié entre-temps. Rechargez la page.",
            "This event was modified in the meantime. Reload the page.",
        ),
        StoreError::Upstream(_) => tr(
            "Google Agenda ne répond pas. Réessayez dans un instant.",
            "Google Calendar is not responding. Try again in a moment.",
        ),
    }
}

/// `409`: the catalog is being modified.
pub fn migrating() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "migrating",
        tr(
            "Les catégories sont en cours de modification. Réessayez dans un instant.",
            "The categories are being modified. Try again in a moment.",
        ),
    )
}

/// Failure of an operation shared by the web API and MCP.
#[derive(Debug, Clone)]
pub enum ServiceError {
    Validation(ValidationError),
    Store(StoreError),
    BadId,
    /// The type colors are being migrated (§19.5).
    Migrating,
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
            ServiceError::BadId => ApiError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                tr("Événement introuvable.", "Event not found."),
            ),
            ServiceError::Migrating => migrating(),
        }
    }
}
