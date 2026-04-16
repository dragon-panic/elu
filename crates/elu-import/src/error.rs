use elu_manifest::ManifestError;
use elu_store::error::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),

    #[error("fetch error: {0}")]
    Fetch(String),

    #[error("archive error: {0}")]
    Archive(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("package not found: {0}")]
    NotFound(String),

    #[error("no matching version for {name}: {detail}")]
    NoVersion { name: String, detail: String },

    #[error("invalid metadata: {0}")]
    InvalidMetadata(String),
}
