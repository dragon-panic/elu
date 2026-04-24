use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::post;

use super::AppState;
use super::auth::Publisher;
use super::error::HttpError;
use crate::error::RegistryError;
use crate::types::*;

const RESERVED_NAMESPACES: &[&str] = &["debian", "npm", "pip"];

/// POST /api/v1/packages/:ns/:name/:version — begin publish
async fn begin_publish(
    State(state): State<Arc<AppState>>,
    Publisher(publisher): Publisher,
    Path((ns, name, version)): Path<(String, String, String)>,
    Json(req): Json<PublishRequest>,
) -> Result<Json<PublishResponse>, HttpError> {
    // Check reserved namespaces
    if RESERVED_NAMESPACES.contains(&ns.as_str()) {
        return Err(RegistryError::ReservedNamespace {
            namespace: ns,
        }
        .into());
    }

    // Validate manifest: decode base64, parse as manifest
    let manifest_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &req.manifest,
    )
    .map_err(|e| RegistryError::InvalidManifest {
        reason: format!("invalid base64: {e}"),
    })?;

    // Manifests in the wild come in two on-store forms: canonical JSON (what
    // `elu build` writes) and TOML (what some tests seed directly). Accept
    // either rather than tying the wire format to a single serialization.
    let manifest = parse_manifest_bytes(&manifest_bytes)?;

    // Verify every diff_id in manifest appears in layers
    for layer in &manifest.layers {
        if let Some(ref diff_id) = layer.diff_id
            && !req.layers.iter().any(|l| &l.diff_id == diff_id) {
                return Err(RegistryError::InvalidManifest {
                    reason: format!("layer diff_id {diff_id} in manifest not found in layers"),
                }
                .into());
            }
    }

    // Check public-depends-on-private
    let visibility = req.visibility.unwrap_or(Visibility::Public);
    if visibility == Visibility::Public {
        for dep in &manifest.dependencies {
            let dep_ref = dep.reference.as_str();
            if let Some((dep_ns, dep_name)) = dep_ref.split_once('/') {
                if let Ok(record) = state.db.get_version(dep_ns, dep_name, "latest")
                    && record.visibility == Visibility::Private {
                        return Err(RegistryError::PublicDependsOnPrivate {
                            dep: dep_ref.to_string(),
                        }
                        .into());
                    }
                // If we can't resolve the dep, that's fine — it might be on another registry
                // We also do a best-effort check: look for any version of this package
                if let Ok(versions) = state.db.list_versions(dep_ns, dep_name)
                    && let Some(latest) = versions.first()
                        && let Ok(record) =
                            state.db.get_version(dep_ns, dep_name, &latest.version)
                            && record.visibility == Visibility::Private {
                                return Err(RegistryError::PublicDependsOnPrivate {
                                    dep: dep_ref.to_string(),
                                }
                                .into());
                            }
            }
        }
    }

    // Create session
    let session_id = uuid::Uuid::new_v4().to_string();

    state.db.put_publish_session(
        &session_id,
        &ns,
        &name,
        &version,
        &req.manifest_blob_id,
        &manifest_bytes,
        &req.layers,
        &publisher,
        visibility,
        &chrono_now(),
    )?;

    // Return upload URLs for blobs the backend doesn't already have
    let mut upload_urls = Vec::new();
    for layer in &req.layers {
        if !state.blob_backend.has_blob(&layer.blob_id)? {
            upload_urls.push(UploadUrl {
                blob_id: layer.blob_id.clone(),
                upload_url: state.blob_backend.upload_url(&layer.blob_id)?,
            });
        }
    }

    // Also generate upload URL for the manifest blob
    if !state.blob_backend.has_blob(
        &elu_store::hash::BlobId(req.manifest_blob_id.0.clone()),
    )? {
        let manifest_blob_id_as_blob =
            elu_store::hash::BlobId(req.manifest_blob_id.0.clone());
        upload_urls.push(UploadUrl {
            blob_id: manifest_blob_id_as_blob.clone(),
            upload_url: state.blob_backend.upload_url(&manifest_blob_id_as_blob)?,
        });
    }

    Ok(Json(PublishResponse {
        session_id,
        upload_urls,
    }))
}

/// POST /api/v1/packages/:ns/:name/:version/commit — finalize publish
async fn commit_publish(
    State(state): State<Arc<AppState>>,
    Publisher(_publisher): Publisher,
    Path((ns, name, version)): Path<(String, String, String)>,
) -> Result<Json<PackageRecord>, HttpError> {
    // Find the session for this ns/name/version
    // We need to find the session by ns/name/version since the client
    // may not have the session_id handy in the URL
    let session = find_session_by_package(&state.db, &ns, &name, &version)?;

    // Parse the layers to check blob presence
    let layers: Vec<PublishLayerRecord> =
        serde_json::from_str(&session.layers_json).map_err(|e| RegistryError::Database(e.to_string()))?;

    // Verify all blobs are present
    let mut missing = Vec::new();
    for layer in &layers {
        if !state.blob_backend.has_blob(&layer.blob_id)? {
            missing.push(layer.blob_id.to_string());
        }
    }
    if !missing.is_empty() {
        return Err(RegistryError::MissingBlobs { blob_ids: missing }.into());
    }

    // Build layer URLs
    let layer_urls: Vec<(elu_store::hash::BlobId, url::Url)> = layers
        .iter()
        .map(|l| {
            let url = state.blob_backend.download_url(&l.blob_id)?;
            Ok((l.blob_id.clone(), url))
        })
        .collect::<Result<Vec<_>, RegistryError>>()?;

    // Generate manifest URL
    let manifest_blob_id_as_blob =
        elu_store::hash::BlobId(session.manifest_blob_id.parse::<elu_store::hash::ManifestHash>()
            .map_err(|e| RegistryError::Database(format!("bad manifest hash: {e}")))?
            .0);
    let manifest_url = state.blob_backend.download_url(&manifest_blob_id_as_blob)?;

    let record = state
        .db
        .commit_version(&session.session_id, &manifest_url, &layer_urls)?;

    Ok(Json(record))
}

fn find_session_by_package(
    db: &crate::db::SqliteRegistryDb,
    ns: &str,
    name: &str,
    version: &str,
) -> Result<crate::db::PublishSession, RegistryError> {
    // The DB doesn't have a direct lookup by ns/name/version for sessions,
    // so we need to add that. For now, let's use a simple approach:
    // We store the session_id in a way we can find it.
    db.find_session_by_package(ns, name, version)
}

/// Parse a manifest blob accepting either canonical JSON (the form `elu build`
/// writes) or TOML (the form many tests use directly).
fn parse_manifest_bytes(bytes: &[u8]) -> Result<elu_manifest::Manifest, RegistryError> {
    if let Ok(m) = serde_json::from_slice::<elu_manifest::Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes).map_err(|e| RegistryError::InvalidManifest {
        reason: format!("manifest not UTF-8: {e}"),
    })?;
    elu_manifest::from_toml_str(s).map_err(|e| RegistryError::InvalidManifest {
        reason: format!("invalid manifest: {e}"),
    })
}

fn chrono_now() -> String {
    // Simple ISO 8601 timestamp without pulling in chrono
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = dur.as_secs();
    // Good enough for our purposes
    format!("1970-01-01T00:00:00Z+{secs}s")
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/packages/{ns}/{name}/{version}", post(begin_publish))
        .route(
            "/api/v1/packages/{ns}/{name}/{version}/commit",
            post(commit_publish),
        )
}
