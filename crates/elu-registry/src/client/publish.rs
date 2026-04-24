//! Client-side publish protocol: begin → upload → commit.
//!
//! The registry never touches blob bytes. The client reads blobs from the
//! local store, sends their distribution records to the registry, receives
//! presigned upload URLs for blobs the registry doesn't already have, PUTs
//! each missing blob directly to its URL, then POSTs the commit.

use elu_store::store::Store;

use crate::client::fallback::RegistryClient;
use crate::error::RegistryError;
use crate::types::{PackageRecord, Visibility};

/// Publish a package (already present in `store` under `ns/name@version`) to
/// the first registry in `client`. Returns the committed `PackageRecord`.
///
/// The `token` becomes the publisher identity via the Bearer auth header.
pub async fn publish_package(
    _client: &RegistryClient,
    _store: &dyn Store,
    _namespace: &str,
    _name: &str,
    _version: &str,
    _token: &str,
    _visibility: Option<Visibility>,
) -> Result<PackageRecord, RegistryError> {
    todo!("publish_package — implement begin → upload → commit")
}
