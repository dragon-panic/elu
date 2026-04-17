use elu_store::hash::{BlobId, DiffId, ManifestHash};
use serde::{Deserialize, Serialize};
use url::Url;

/// Distribution record for a single layer, bridging manifest diff_ids to fetchable blobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayerRecord {
    pub diff_id: DiffId,
    pub blob_id: BlobId,
    pub url: Url,
    pub size_compressed: u64,
    pub size_uncompressed: u64,
}

/// Package visibility.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

/// A fully committed package version record as returned by the registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageRecord {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub manifest_blob_id: ManifestHash,
    pub manifest_url: Url,
    pub kind: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub layers: Vec<LayerRecord>,
    pub publisher: String,
    pub published_at: String,
    pub signature: Option<String>,
    pub visibility: Visibility,
}

/// Layer info sent by the client during publish (no URL yet).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishLayerRecord {
    pub diff_id: DiffId,
    pub blob_id: BlobId,
    pub size_compressed: u64,
    pub size_uncompressed: u64,
}

/// Request body for `POST /api/v1/packages/<ns>/<name>/<version>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishRequest {
    pub manifest_blob_id: ManifestHash,
    pub manifest: String, // base64-encoded manifest bytes
    pub layers: Vec<PublishLayerRecord>,
    pub visibility: Option<Visibility>,
}

/// Upload URL for a blob the registry doesn't already have.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadUrl {
    pub blob_id: BlobId,
    pub upload_url: Url,
}

/// Response from `POST /api/v1/packages/<ns>/<name>/<version>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResponse {
    pub session_id: String,
    pub upload_urls: Vec<UploadUrl>,
}

/// Response from `GET /api/v1/packages/<ns>/<name>` (version listing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionListResponse {
    pub namespace: String,
    pub name: String,
    pub versions: Vec<VersionEntry>,
}

/// A single entry in the version list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: String,
    pub published_at: String,
    pub kind: Option<String>,
}

/// Query parameters for `GET /api/v1/search`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub namespace: Option<String>,
}

/// Response from search endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

/// A single search result (latest version of a matching package).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub kind: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub published_at: String,
}

/// Namespace info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceInfo {
    pub namespace: String,
    pub owner: String,
    pub verified: bool,
    pub created_at: String,
}
