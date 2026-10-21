//! HTTP router.

use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router, middleware};
use serde_json::{Value, json};
use tower_http::compression::CompressionLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::state::AppState;
use crate::{api, google_login, i18n, mcp, oauth, pages, security, session, web};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/assets/{*path}", get(asset))
        .nest("/api", api::router(state.clone()))
        .merge(pages::router(state.clone()))
        .merge(google_login::router(state.clone()))
        .merge(mcp::router(state.clone()))
        .merge(oauth::router(state.clone()))
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            session::renew_session,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            i18n::lang_layer,
        ))
        .layer(middleware::from_fn(security::security_headers))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http().on_response(DefaultOnResponse::new().level(Level::INFO)))
        .with_state(state)
}

async fn healthz(State(s): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "mode": s.mode.as_str(),
        "default_language": s.config.default_lang(),
    }))
}

async fn asset(Path(path): Path<String>) -> Response {
    web::serve(&format!("assets/{path}"))
}

async fn not_found() -> Response {
    web::not_found()
}
