use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::get;

use super::AppState;
use super::auth::OptionalPublisher;
use super::error::HttpError;
use crate::types::*;

/// GET /api/v1/packages/:ns/:name/:version — package record
async fn get_package(
    State(state): State<Arc<AppState>>,
    OptionalPublisher(publisher): OptionalPublisher,
    Path((ns, name, version)): Path<(String, String, String)>,
) -> Result<Json<PackageRecord>, HttpError> {
    let record = state
        .db
        .get_version_with_visibility(&ns, &name, &version, publisher.as_deref())?;
    Ok(Json(record))
}

/// GET /api/v1/packages/:ns/:name — version list
async fn list_versions(
    State(state): State<Arc<AppState>>,
    OptionalPublisher(publisher): OptionalPublisher,
    Path((ns, name)): Path<(String, String)>,
) -> Result<Json<VersionListResponse>, HttpError> {
    let versions = state
        .db
        .list_versions_with_visibility(&ns, &name, publisher.as_deref())?;
    Ok(Json(VersionListResponse {
        namespace: ns,
        name,
        versions,
    }))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/packages/{ns}/{name}/{version}", get(get_package))
        .route("/api/v1/packages/{ns}/{name}", get(list_versions))
}
