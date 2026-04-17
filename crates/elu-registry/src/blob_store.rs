use std::collections::HashSet;
use std::sync::Mutex;

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

/// A local blob backend that generates URLs pointing at a local HTTP server.
/// Suitable for dev/test and small self-hosted deployments.
pub struct LocalBlobBackend {
    base_url: Url,
    uploaded: Mutex<HashSet<String>>,
}

impl LocalBlobBackend {
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            uploaded: Mutex::new(HashSet::new()),
        }
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

    fn has_blob(&self, blob_id: &BlobId) -> Result<bool, RegistryError> {
        let uploaded = self.uploaded.lock().unwrap();
        Ok(uploaded.contains(&blob_id.to_string()))
    }

    fn mark_uploaded(&self, blob_id: &BlobId) -> Result<(), RegistryError> {
        let mut uploaded = self.uploaded.lock().unwrap();
        uploaded.insert(blob_id.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elu_store::hash::{Hash, HashAlgo};

    fn test_blob_id(b: u8) -> BlobId {
        BlobId(Hash::new(HashAlgo::Sha256, [b; 32]))
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
    fn local_backend_has_blob_after_mark() {
        let backend = LocalBlobBackend::new(Url::parse("http://localhost:8080/").unwrap());
        let blob_id = test_blob_id(0xbb);

        assert!(!backend.has_blob(&blob_id).unwrap());
        backend.mark_uploaded(&blob_id).unwrap();
        assert!(backend.has_blob(&blob_id).unwrap());
    }
}
