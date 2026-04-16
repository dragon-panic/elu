pub mod apt;
pub mod cache;
pub mod error;
pub mod fetch;
pub mod manifest_build;
pub mod npm;
pub mod pip;
pub mod tar_layer;

use elu_store::hash::ManifestHash;
use elu_store::store::Store;

use crate::cache::Cache;
use crate::error::ImportError;
use crate::fetch::Fetcher;

/// Options controlling import behavior.
#[derive(Default)]
pub struct ImportOptions {
    /// Specific version to import. None means latest.
    pub version: Option<String>,
    /// Whether to transitively import all dependencies.
    pub closure: bool,
    /// apt: distribution (e.g. "bookworm", "jammy").
    pub dist: Option<String>,
    /// pip: platform tag (e.g. "py3-none-any").
    pub target: Option<String>,
}

/// Common interface for all importers.
pub trait Importer {
    fn import(
        &self,
        name: &str,
        options: &ImportOptions,
        store: &dyn Store,
        cache: &Cache,
        fetcher: &dyn Fetcher,
    ) -> Result<ManifestHash, ImportError>;
}
