use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::get;
use elu_store::hash::ManifestHash;

use super::AppState;
use super::auth::OptionalPublisher;
use super::error::HttpError;
use crate::error::RegistryError;
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

/// GET /api/v1/manifests/:hash — package record keyed by manifest hash
async fn get_by_manifest_hash(
    State(state): State<Arc<AppState>>,
    OptionalPublisher(publisher): OptionalPublisher,
    Path(hash): Path<String>,
) -> Result<Json<PackageRecord>, HttpError> {
    let parsed: ManifestHash = hash
        .parse()
        .map_err(|_| RegistryError::ManifestHashNotFound { hash: hash.clone() })?;
    let record = state
        .db
        .get_version_by_manifest_hash_with_visibility(&parsed, publisher.as_deref())?;
    Ok(Json(record))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/packages/{ns}/{name}/{version}", get(get_package))
        .route("/api/v1/packages/{ns}/{name}", get(list_versions))
        .route("/api/v1/manifests/{hash}", get(get_by_manifest_hash))
}
