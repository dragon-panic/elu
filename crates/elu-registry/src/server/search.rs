use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get;

use super::AppState;
use super::auth::OptionalPublisher;
use super::error::HttpError;
use crate::types::*;

/// GET /api/v1/search?q=...&kind=...&tag=...&namespace=...
async fn search(
    State(state): State<Arc<AppState>>,
    OptionalPublisher(publisher): OptionalPublisher,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, HttpError> {
    let results = state.db.search(&query, publisher.as_deref())?;
    Ok(Json(SearchResponse { results }))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/search", get(search))
}
