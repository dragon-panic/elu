//! Client-side publish protocol: begin → upload → commit.
//!
//! The registry never touches blob bytes. The client reads blobs from the
//! local store, sends their distribution records to the registry, receives
//! presigned upload URLs for blobs the registry doesn't already have, PUTs
//! each missing blob directly to its URL, then POSTs the commit.

use base64::Engine;
use elu_manifest::Manifest;
use elu_store::hash::{BlobId, ManifestHash};
use elu_store::store::Store;

use crate::client::fallback::RegistryClient;
use crate::error::RegistryError;
use crate::types::{
    PackageRecord, PublishLayerRecord, PublishRequest, PublishResponse, Visibility,
};

/// Publish a package (already present in `store` under `ns/name@version`) to
/// the first registry in `client`. Returns the committed `PackageRecord`.
///
/// The `token` becomes the publisher identity via the Bearer auth header.
pub async fn publish_package(
    client: &RegistryClient,
    store: &dyn Store,
    namespace: &str,
    name: &str,
    version: &str,
    token: &str,
    visibility: Option<Visibility>,
) -> Result<PackageRecord, RegistryError> {
    let base = client
        .registries()
        .first()
        .ok_or_else(|| RegistryError::InvalidManifest {
            reason: "no registries configured".into(),
        })?
        .clone();

    // 1. Resolve manifest by ref.
    let manifest_hash = store
        .get_ref(namespace, name, version)
        .map_err(store_err)?
        .ok_or_else(|| RegistryError::VersionNotFound {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        })?;

    let manifest_bytes = store
        .get_manifest(&manifest_hash)
        .map_err(store_err)?
        .ok_or_else(|| RegistryError::InvalidManifest {
            reason: format!("manifest {manifest_hash} missing from store"),
        })?;

    // 2. Parse the manifest and gather its layers' distribution records.
    // The store may hold either canonical-JSON (what `elu build` writes) or
    // TOML (what `client_publish` tests seed); accept either.
    let manifest: Manifest = parse_manifest_bytes(&manifest_bytes)?;

    let mut layers = Vec::with_capacity(manifest.layers.len());
    for (idx, layer) in manifest.layers.iter().enumerate() {
        let diff_id = layer.diff_id.clone().ok_or_else(|| RegistryError::InvalidManifest {
            reason: format!("layer {idx} has no diff_id (source-form not publishable)"),
        })?;
        let size_uncompressed = layer.size.ok_or_else(|| RegistryError::InvalidManifest {
            reason: format!("layer {idx} has no size"),
        })?;

        let blob_id = store
            .resolve_diff(&diff_id)
            .map_err(store_err)?
            .ok_or_else(|| RegistryError::InvalidManifest {
                reason: format!("layer {idx}: no blob for diff_id {diff_id}"),
            })?;

        let size_compressed = store
            .size(&blob_id)
            .map_err(store_err)?
            .ok_or_else(|| RegistryError::InvalidManifest {
                reason: format!("layer {idx}: blob {blob_id} missing"),
            })?;

        layers.push(PublishLayerRecord {
            diff_id,
            blob_id,
            size_compressed,
            size_uncompressed,
        });
    }

    // 3. POST begin.
    let manifest_b64 = base64::engine::general_purpose::STANDARD.encode(&manifest_bytes);
    let begin_url = base
        .join(&format!(
            "api/v1/packages/{namespace}/{name}/{version}",
        ))
        .map_err(|e| RegistryError::BlobBackend(e.to_string()))?;

    let req_body = PublishRequest {
        manifest_blob_id: manifest_hash.clone(),
        manifest: manifest_b64,
        layers,
        visibility,
    };

    let http = reqwest::Client::new();
    let resp = http
        .post(begin_url)
        .bearer_auth(token)
        .json(&req_body)
        .send()
        .await
        .map_err(|e| RegistryError::BlobBackend(format!("begin publish: {e}")))?;
    if !resp.status().is_success() {
        return Err(RegistryError::BlobBackend(format!(
            "begin publish returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default(),
        )));
    }
    let publish_resp: PublishResponse = resp
        .json()
        .await
        .map_err(|e| RegistryError::InvalidManifest {
            reason: format!("publish response: {e}"),
        })?;

    // 4. PUT each missing blob to its upload_url. The server may also include
    //    a slot for the manifest blob (its blob_id is the manifest hash bytes);
    //    we serve those bytes from `get_manifest`.
    for upload in &publish_resp.upload_urls {
        let bytes = blob_bytes_for_upload(store, &upload.blob_id, &manifest_hash, &manifest_bytes)?;
        let resp = http
            .put(upload.upload_url.clone())
            .bearer_auth(token)
            .body(bytes)
            .send()
            .await
            .map_err(|e| RegistryError::BlobBackend(format!("upload {}: {e}", upload.blob_id)))?;
        if !resp.status().is_success() {
            return Err(RegistryError::BlobBackend(format!(
                "upload {} returned {}",
                upload.blob_id,
                resp.status(),
            )));
        }
    }

    // 5. POST commit.
    let commit_url = base
        .join(&format!(
            "api/v1/packages/{namespace}/{name}/{version}/commit",
        ))
        .map_err(|e| RegistryError::BlobBackend(e.to_string()))?;
    let resp = http
        .post(commit_url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| RegistryError::BlobBackend(format!("commit: {e}")))?;
    if !resp.status().is_success() {
        return Err(RegistryError::BlobBackend(format!(
            "commit returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default(),
        )));
    }
    let record: PackageRecord = resp.json().await.map_err(|e| RegistryError::InvalidManifest {
        reason: format!("commit response: {e}"),
    })?;
    Ok(record)
}

/// Read the bytes for one upload slot. The server returns one slot per missing
/// blob plus optionally the manifest blob; the manifest's blob_id has the same
/// hash bytes as the manifest hash, so we pick the right source by comparing.
fn blob_bytes_for_upload(
    store: &dyn Store,
    blob_id: &BlobId,
    manifest_hash: &ManifestHash,
    manifest_bytes: &[u8],
) -> Result<Vec<u8>, RegistryError> {
    if blob_id.0 == manifest_hash.0 {
        return Ok(manifest_bytes.to_vec());
    }
    let bytes = store
        .get(blob_id)
        .map_err(store_err)?
        .ok_or_else(|| RegistryError::BlobBackend(format!("blob {blob_id} missing from store")))?;
    Ok(bytes.to_vec())
}

fn store_err(e: elu_store::error::StoreError) -> RegistryError {
    RegistryError::BlobBackend(format!("store: {e}"))
}

/// Parse a manifest from store bytes. Accepts canonical JSON (the form `elu
/// build` writes) and TOML (the form some tests seed directly).
fn parse_manifest_bytes(bytes: &[u8]) -> Result<Manifest, RegistryError> {
    if let Ok(m) = serde_json::from_slice::<Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes).map_err(|e| RegistryError::InvalidManifest {
        reason: format!("manifest not UTF-8: {e}"),
    })?;
    elu_manifest::from_toml_str(s).map_err(|e| RegistryError::InvalidManifest {
        reason: format!("invalid manifest: {e}"),
    })
}
