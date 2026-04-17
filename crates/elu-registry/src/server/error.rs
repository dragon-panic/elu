use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::error::RegistryError;

/// Wraps RegistryError for HTTP responses.
pub struct HttpError(pub RegistryError);

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self.0 {
            RegistryError::VersionExists { .. } => (StatusCode::CONFLICT, self.0.to_string()),
            RegistryError::VersionNotFound { .. } => (StatusCode::NOT_FOUND, self.0.to_string()),
            RegistryError::PackageNotFound { .. } => (StatusCode::NOT_FOUND, self.0.to_string()),
            RegistryError::SessionNotFound { .. } => (StatusCode::NOT_FOUND, self.0.to_string()),
            RegistryError::MissingBlobs { .. } => {
                (StatusCode::PRECONDITION_FAILED, self.0.to_string())
            }
            RegistryError::InvalidManifest { .. } => {
                (StatusCode::BAD_REQUEST, self.0.to_string())
            }
            RegistryError::ReservedNamespace { .. } => {
                (StatusCode::FORBIDDEN, self.0.to_string())
            }
            RegistryError::NamespaceNotFound { .. } => {
                (StatusCode::NOT_FOUND, self.0.to_string())
            }
            RegistryError::NamespaceAlreadyClaimed { .. } => {
                (StatusCode::CONFLICT, self.0.to_string())
            }
            RegistryError::NotAuthorized => (StatusCode::FORBIDDEN, self.0.to_string()),
            RegistryError::PublicDependsOnPrivate { .. } => {
                (StatusCode::BAD_REQUEST, self.0.to_string())
            }
            RegistryError::Database(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
            RegistryError::BlobBackend(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, msg).into_response()
    }
}

impl From<RegistryError> for HttpError {
    fn from(e: RegistryError) -> Self {
        HttpError(e)
    }
}
