use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("version already exists: {namespace}/{name}@{version}")]
    VersionExists {
        namespace: String,
        name: String,
        version: String,
    },

    #[error("version not found: {namespace}/{name}@{version}")]
    VersionNotFound {
        namespace: String,
        name: String,
        version: String,
    },

    #[error("manifest hash not found: {hash}")]
    ManifestHashNotFound { hash: String },

    #[error("package not found: {namespace}/{name}")]
    PackageNotFound { namespace: String, name: String },

    #[error("session not found: {session_id}")]
    SessionNotFound { session_id: String },

    #[error("missing blobs: {blob_ids:?}")]
    MissingBlobs { blob_ids: Vec<String> },

    #[error("invalid manifest: {reason}")]
    InvalidManifest { reason: String },

    #[error("reserved namespace: {namespace}")]
    ReservedNamespace { namespace: String },

    #[error("namespace not found: {namespace}")]
    NamespaceNotFound { namespace: String },

    #[error("namespace already claimed: {namespace}")]
    NamespaceAlreadyClaimed { namespace: String },

    #[error("not authorized")]
    NotAuthorized,

    #[error("public package cannot depend on private package: {dep}")]
    PublicDependsOnPrivate { dep: String },

    #[error("database error: {0}")]
    Database(String),

    #[error("blob backend error: {0}")]
    BlobBackend(String),
}

impl From<rusqlite::Error> for RegistryError {
    fn from(e: rusqlite::Error) -> Self {
        RegistryError::Database(e.to_string())
    }
}
