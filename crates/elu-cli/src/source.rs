//! `HybridSource`: composes `OfflineSource` (local store) with
//! `RegistrySource` (HTTP). Looks up locally first; falls back to the
//! registry for anything missing. `list_versions` returns the union so
//! the resolver picks the highest match across both views.
//!
//! When `--offline` is set, callers build a hybrid with `registry: None`
//! and the source degrades to pure offline behavior — same semantics as
//! `OfflineSource` alone, but uniform call site for the verbs that drive
//! the resolver.

use std::sync::Arc;

use elu_manifest::types::PackageRef;
use elu_registry::source::RegistrySource;
use elu_resolver::error::ResolverError;
use elu_resolver::source::{FetchedManifest, OfflineSource, VersionSource};
use elu_store::hash::ManifestHash;
use semver::Version;

pub struct HybridSource {
    pub offline: OfflineSource,
    pub registry: Option<Arc<RegistrySource>>,
}

impl HybridSource {
    pub fn new(offline: OfflineSource, registry: Option<Arc<RegistrySource>>) -> Self {
        Self { offline, registry }
    }
}

impl VersionSource for HybridSource {
    async fn list_versions(
        &self,
        package: &PackageRef,
    ) -> Result<Vec<Version>, ResolverError> {
        let mut local = self.offline.list_versions(package).await?;
        if let Some(reg) = &self.registry {
            // Registry errors here are non-fatal: a not-found (or transport
            // hiccup) shouldn't kill resolution if the offline source covers
            // the constraint. The resolver will surface NoMatch if the union
            // ends up empty.
            if let Ok(remote) = reg.list_versions(package).await {
                for v in remote {
                    if !local.contains(&v) {
                        local.push(v);
                    }
                }
            }
        }
        local.sort();
        Ok(local)
    }

    async fn fetch_manifest(
        &self,
        package: &PackageRef,
        version: &Version,
    ) -> Result<FetchedManifest, ResolverError> {
        if let Ok(m) = self.offline.fetch_manifest(package, version).await {
            return Ok(m);
        }
        match &self.registry {
            Some(reg) => reg.fetch_manifest(package, version).await,
            None => Err(ResolverError::Source(format!(
                "{package}@{version} not in local store and registry source is disabled (--offline)",
            ))),
        }
    }

    async fn fetch_by_hash(
        &self,
        hash: &ManifestHash,
    ) -> Result<FetchedManifest, ResolverError> {
        if let Ok(m) = self.offline.fetch_by_hash(hash).await {
            return Ok(m);
        }
        match &self.registry {
            Some(reg) => reg.fetch_by_hash(hash).await,
            None => Err(ResolverError::Source(format!(
                "manifest {hash} not in local store and registry source is disabled (--offline)",
            ))),
        }
    }
}
