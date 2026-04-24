use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::put;
use elu_store::hash::BlobId;
use url::Url;

use crate::error::RegistryError;

/// Trait for blob storage backends. The registry never touches blob bytes;
/// this trait generates presigned URLs for clients to upload/download directly.
pub trait BlobBackend: Send + Sync {
    fn upload_url(&self, blob_id: &BlobId) -> Result<Url, RegistryError>;
    fn download_url(&self, blob_id: &BlobId) -> Result<Url, RegistryError>;
    fn has_blob(&self, blob_id: &BlobId) -> Result<bool, RegistryError>;
    fn mark_uploaded(&self, blob_id: &BlobId) -> Result<(), RegistryError>;
}

/// A local blob backend that persists bytes in memory and serves them over
/// HTTP via [`LocalBlobBackend::router`]. Suitable for dev/test and small
/// self-hosted deployments. Production deployments should use S3/GCS-backed
/// implementations of [`BlobBackend`] that issue real presigned URLs.
pub struct LocalBlobBackend {
    base_url: Url,
    blobs: Mutex<HashMap<BlobId, Vec<u8>>>,
    /// Blobs known to be present at the backend without our holding the bytes
    /// — used by S3/GCS-style flows and by server-logic tests that signal
    /// uploads without going through the byte-serving path.
    marked: Mutex<HashSet<BlobId>>,
}

impl LocalBlobBackend {
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            blobs: Mutex::new(HashMap::new()),
            marked: Mutex::new(HashSet::new()),
        }
    }

    /// Store `bytes` under `blob_id`, after verifying the bytes hash to it.
    /// Returns `BlobBackend("hash mismatch")` on mismatch.
    pub fn put(&self, blob_id: &BlobId, bytes: Vec<u8>) -> Result<(), RegistryError> {
        let mut hasher = elu_store::hasher::Hasher::new();
        hasher.update(&bytes);
        let actual = BlobId(hasher.finalize());
        if &actual != blob_id {
            return Err(RegistryError::BlobBackend(format!(
                "hash mismatch: expected {blob_id}, got {actual}"
            )));
        }
        self.blobs
            .lock()
            .unwrap()
            .insert(blob_id.clone(), bytes);
        self.marked.lock().unwrap().insert(blob_id.clone());
        Ok(())
    }

    /// Fetch the stored bytes for `blob_id`, or `Ok(None)` if absent.
    pub fn get(&self, blob_id: &BlobId) -> Result<Option<Vec<u8>>, RegistryError> {
        Ok(self.blobs.lock().unwrap().get(blob_id).cloned())
    }

    /// Build an `axum::Router` exposing `PUT /blobs/{blob_id}` and
    /// `GET /blobs/{blob_id}` over `self`. Callers spawn whichever listener
    /// fits their deployment shape; the URLs the backend hands out via
    /// [`BlobBackend::upload_url`] / [`BlobBackend::download_url`] should
    /// resolve to wherever this router is mounted.
    pub fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route(
                "/blobs/{blob_id}",
                put(put_blob_handler).get(get_blob_handler),
            )
            .with_state(self)
    }
}

/// PUT /blobs/{blob_id} — verify body hashes to `{blob_id}`, then persist.
async fn put_blob_handler(
    State(backend): State<Arc<LocalBlobBackend>>,
    Path(blob_id_str): Path<String>,
    body: Body,
) -> StatusCode {
    let blob_id: BlobId = match blob_id_str.parse() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    // 1 GiB cap is plenty for the dev/test/self-host tier this backend
    // targets — production deployments use real object stores with their
    // own size limits.
    let bytes = match to_bytes(body, 1024 * 1024 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    match backend.put(&blob_id, bytes) {
        Ok(()) => StatusCode::OK,
        // The only failure mode `put` has today is a hash mismatch, which is
        // a client-side bug — surface it as 400.
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

/// GET /blobs/{blob_id} — return stored bytes or 404.
async fn get_blob_handler(
    State(backend): State<Arc<LocalBlobBackend>>,
    Path(blob_id_str): Path<String>,
) -> Response {
    let blob_id: BlobId = match blob_id_str.parse() {
        Ok(b) => b,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
        }
    };
    match backend.get(&blob_id) {
        Ok(Some(bytes)) => Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(bytes))
            .unwrap(),
        Ok(None) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap(),
    }
}

impl BlobBackend for LocalBlobBackend {
    fn upload_url(&self, blob_id: &BlobId) -> Result<Url, RegistryError> {
        self.base_url
            .join(&format!("blobs/{}", blob_id))
            .map_err(|e| RegistryError::BlobBackend(e.to_string()))
    }

    fn download_url(&self, blob_id: &BlobId) -> Result<Url, RegistryError> {
        self.base_url
            .join(&format!("blobs/{}", blob_id))
            .map_err(|e| RegistryError::BlobBackend(e.to_string()))
    }

    /// True if the backend either holds the bytes locally (via `put` or the
    /// router's PUT) or has been told the upload happened externally (via
    /// `mark_uploaded` — for tests that exercise server commit logic without
    /// running the full upload protocol, and for S3/GCS-style backends where
    /// the registry never sees bytes directly).
    fn has_blob(&self, blob_id: &BlobId) -> Result<bool, RegistryError> {
        if self.blobs.lock().unwrap().contains_key(blob_id) {
            return Ok(true);
        }
        Ok(self.marked.lock().unwrap().contains(blob_id))
    }

    fn mark_uploaded(&self, blob_id: &BlobId) -> Result<(), RegistryError> {
        self.marked.lock().unwrap().insert(blob_id.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use elu_store::hash::{Hash, HashAlgo};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_blob_id(b: u8) -> BlobId {
        BlobId(Hash::new(HashAlgo::Sha256, [b; 32]))
    }

    /// Hash `bytes` to produce a real BlobId so PUT verification accepts it.
    fn blob_id_of(bytes: &[u8]) -> BlobId {
        let mut hasher = elu_store::hasher::Hasher::new();
        hasher.update(bytes);
        BlobId(hasher.finalize())
    }

    #[test]
    fn local_backend_upload_and_download_urls() {
        let backend = LocalBlobBackend::new(Url::parse("http://localhost:8080/").unwrap());
        let blob_id = test_blob_id(0xaa);

        let upload = backend.upload_url(&blob_id).unwrap();
        let download = backend.download_url(&blob_id).unwrap();

        assert!(upload.as_str().contains("blobs/"));
        assert_eq!(upload, download);
    }

    #[test]
    fn put_then_get_round_trip() {
        let backend = LocalBlobBackend::new(Url::parse("http://localhost:8080/").unwrap());
        let bytes = b"hello blob".to_vec();
        let blob_id = blob_id_of(&bytes);

        assert!(!backend.has_blob(&blob_id).unwrap());
        backend.put(&blob_id, bytes.clone()).unwrap();
        assert!(backend.has_blob(&blob_id).unwrap());
        assert_eq!(backend.get(&blob_id).unwrap(), Some(bytes));
    }

    #[test]
    fn put_with_wrong_hash_is_rejected() {
        let backend = LocalBlobBackend::new(Url::parse("http://localhost:8080/").unwrap());
        let wrong_id = test_blob_id(0x00); // zeros — will not match real hash
        let bytes = b"some bytes".to_vec();
        let err = backend.put(&wrong_id, bytes).unwrap_err();
        assert!(
            matches!(err, RegistryError::BlobBackend(ref m) if m.contains("hash mismatch")),
            "expected hash-mismatch error, got: {err:?}"
        );
        assert!(!backend.has_blob(&wrong_id).unwrap());
    }

    #[tokio::test]
    async fn router_put_then_get_round_trip() {
        let backend = Arc::new(LocalBlobBackend::new(
            Url::parse("http://localhost/").unwrap(),
        ));
        let app = backend.clone().router();

        let body = b"hello router".to_vec();
        let blob_id = blob_id_of(&body);
        let path = format!("/blobs/{blob_id}");

        let put_req = Request::builder()
            .method("PUT")
            .uri(&path)
            .body(Body::from(body.clone()))
            .unwrap();
        let put_resp = app.clone().oneshot(put_req).await.unwrap();
        assert_eq!(put_resp.status(), StatusCode::OK);

        let get_req = Request::builder()
            .method("GET")
            .uri(&path)
            .body(Body::empty())
            .unwrap();
        let get_resp = app.oneshot(get_req).await.unwrap();
        assert_eq!(get_resp.status(), StatusCode::OK);
        let got = get_resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(got.as_ref(), body.as_slice());
    }

    #[tokio::test]
    async fn router_put_with_wrong_hash_returns_400() {
        let backend = Arc::new(LocalBlobBackend::new(
            Url::parse("http://localhost/").unwrap(),
        ));
        let app = backend.router();

        // Path advertises an all-zeros BlobId, but the body hashes to
        // something else — server must reject.
        let zero_id = test_blob_id(0x00);
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/blobs/{zero_id}"))
            .body(Body::from("not the right bytes".as_bytes().to_vec()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn router_get_missing_returns_404() {
        let backend = Arc::new(LocalBlobBackend::new(
            Url::parse("http://localhost/").unwrap(),
        ));
        let app = backend.router();

        let missing = test_blob_id(0xcc);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/blobs/{missing}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
