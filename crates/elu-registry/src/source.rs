//! Registry-backed `VersionSource` impl.
//!
//! Lives in `elu-registry` (not `elu-resolver`) per the WKIW.0CZW ring
//! cleanup: resolver knows about a trait; impls live in the crate that owns
//! the fetching mechanism. The hybrid (store-first, registry-fallback)
//! source that real CLI verbs use is composed in `elu-cli`; this crate
//! exports only the registry leg.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use elu_manifest::Manifest;
use elu_manifest::types::PackageRef;
use elu_resolver::error::ResolverError;
use elu_resolver::source::{FetchedManifest, VersionSource};
use elu_store::hash::ManifestHash;
use semver::Version;

use crate::client::fallback::RegistryClient;
use crate::client::verify::verify_manifest;
use crate::error::RegistryError;
use crate::types::PackageRecord;

/// `VersionSource` backed by an HTTP registry.
///
/// Caches fetched `PackageRecord`s by `(ns, name, version)` and by manifest
/// hash so the install-side fetch loop can find each layer's `(blob_id, url)`
/// after `resolve()` returns. Without the cache, the caller would have to
/// re-fetch the record for every layer in the plan.
pub struct RegistrySource {
    client: Arc<RegistryClient>,
    cache: Mutex<Cache>,
}

#[derive(Default)]
struct Cache {
    by_named: BTreeMap<(String, String, String), PackageRecord>,
    by_hash: HashMap<ManifestHash, PackageRecord>,
}

impl RegistrySource {
    pub fn new(client: Arc<RegistryClient>) -> Self {
        Self {
            client,
            cache: Mutex::new(Cache::default()),
        }
    }

    /// Look up a layer's distribution record (blob_id + download URL) by its
    /// diff_id. The install-side caller walks the resolver's fetch plan and
    /// uses this to turn `FetchKind::Layer(diff_id)` into a real download.
    /// Returns `None` if no cached record covers this diff_id (caller should
    /// treat that as an internal error — every layer in the plan came from
    /// some manifest the resolver fetched, so its record must be cached).
    pub fn layer_record_for_diff(
        &self,
        diff_id: &elu_store::hash::DiffId,
    ) -> Option<crate::types::LayerRecord> {
        let cache = self.cache.lock().unwrap();
        for record in cache.by_named.values() {
            for layer in &record.layers {
                if &layer.diff_id == diff_id {
                    return Some(layer.clone());
                }
            }
        }
        None
    }

    /// Look up a manifest's distribution URL by its hash. Returns `None` if
    /// the resolver never fetched a record covering that hash.
    pub fn manifest_url_for_hash(&self, hash: &ManifestHash) -> Option<url::Url> {
        let cache = self.cache.lock().unwrap();
        cache
            .by_hash
            .get(hash)
            .map(|r| r.manifest_url.clone())
    }

    fn cache_record(&self, record: PackageRecord) {
        let mut cache = self.cache.lock().unwrap();
        cache.by_hash.insert(record.manifest_blob_id.clone(), record.clone());
        cache.by_named.insert(
            (record.namespace.clone(), record.name.clone(), record.version.clone()),
            record,
        );
    }

    async fn fetch_and_cache_manifest(
        &self,
        record: PackageRecord,
    ) -> Result<FetchedManifest, ResolverError> {
        let manifest_bytes = self
            .client
            .fetch_bytes(&record.manifest_url)
            .await
            .map_err(reg_to_resolver)?;
        verify_manifest(&manifest_bytes, &record.manifest_blob_id).map_err(reg_to_resolver)?;
        let manifest = parse_manifest(&manifest_bytes)?;

        let mut layer_urls = BTreeMap::new();
        for layer in &record.layers {
            layer_urls.insert(layer.diff_id.to_string(), layer.url.clone());
        }
        let fetched = FetchedManifest {
            hash: record.manifest_blob_id.clone(),
            manifest,
            manifest_url: Some(record.manifest_url.clone()),
            layer_urls,
        };
        self.cache_record(record);
        Ok(fetched)
    }
}

impl VersionSource for RegistrySource {
    async fn list_versions(
        &self,
        package: &PackageRef,
    ) -> Result<Vec<Version>, ResolverError> {
        let (ns, name) = split_pkg(package);
        let response = self
            .client
            .list_versions(ns, name)
            .await
            .map_err(reg_to_resolver)?;
        let mut out: Vec<Version> = response
            .versions
            .into_iter()
            .filter_map(|entry| entry.version.parse::<Version>().ok())
            .collect();
        out.sort();
        Ok(out)
    }

    async fn fetch_manifest(
        &self,
        package: &PackageRef,
        version: &Version,
    ) -> Result<FetchedManifest, ResolverError> {
        let (ns, name) = split_pkg(package);
        let version_str = version.to_string();

        let cached = {
            let cache = self.cache.lock().unwrap();
            cache
                .by_named
                .get(&(ns.to_string(), name.to_string(), version_str.clone()))
                .cloned()
        };
        if let Some(record) = cached {
            return self.fetch_and_cache_manifest(record).await;
        }

        let record = self
            .client
            .fetch_package(ns, name, &version_str)
            .await
            .map_err(reg_to_resolver)?;
        self.fetch_and_cache_manifest(record).await
    }

    async fn fetch_by_hash(
        &self,
        hash: &ManifestHash,
    ) -> Result<FetchedManifest, ResolverError> {
        let cached = {
            let cache = self.cache.lock().unwrap();
            cache.by_hash.get(hash).cloned()
        };
        if let Some(record) = cached {
            return self.fetch_and_cache_manifest(record).await;
        }
        let record = self
            .client
            .fetch_package_by_hash(hash)
            .await
            .map_err(reg_to_resolver)?;
        self.fetch_and_cache_manifest(record).await
    }
}

fn split_pkg(p: &PackageRef) -> (&str, &str) {
    p.as_str().split_once('/').expect("PackageRef invariant: contains '/'")
}

fn parse_manifest(bytes: &[u8]) -> Result<Manifest, ResolverError> {
    if let Ok(m) = serde_json::from_slice::<Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes)
        .map_err(|_| ResolverError::ManifestDecode("manifest is not utf-8".into()))?;
    elu_manifest::from_toml_str(s)
        .map_err(|e| ResolverError::ManifestDecode(e.to_string()))
}

fn reg_to_resolver(e: RegistryError) -> ResolverError {
    ResolverError::Source(e.to_string())
}
