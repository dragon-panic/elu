use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;

use super::AppState;
use super::auth::Publisher;
use super::error::HttpError;
use crate::types::*;

/// GET /api/v1/namespaces/:ns — namespace info
async fn get_namespace(
    State(state): State<Arc<AppState>>,
    Path(ns): Path<String>,
) -> Result<Json<NamespaceInfo>, HttpError> {
    let info = state.db.get_namespace(&ns)?;
    Ok(Json(info))
}

/// POST /api/v1/namespaces/:ns — claim namespace
async fn claim_namespace(
    State(state): State<Arc<AppState>>,
    Publisher(publisher): Publisher,
    Path(ns): Path<String>,
) -> Result<(StatusCode, Json<NamespaceInfo>), HttpError> {
    let info = NamespaceInfo {
        namespace: ns,
        owner: publisher,
        verified: false,
        created_at: "2026-01-01T00:00:00Z".into(), // simplified
    };
    state.db.put_namespace(&info)?;
    Ok((StatusCode::CREATED, Json(info)))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/namespaces/{ns}", get(get_namespace).post(claim_namespace))
}
