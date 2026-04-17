pub mod auth;
pub mod error;
pub mod fetch;
pub mod namespace;
pub mod publish;
pub mod search;

use std::sync::Arc;

use axum::Router;

use crate::blob_store::BlobBackend;
use crate::db::SqliteRegistryDb;

/// Shared application state for all handlers.
pub struct AppState {
    pub db: SqliteRegistryDb,
    pub blob_backend: Arc<dyn BlobBackend>,
}

/// Build the axum router with all registry endpoints.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(publish::routes())
        .merge(fetch::routes())
        .merge(search::routes())
        .merge(namespace::routes())
        .with_state(state)
}
