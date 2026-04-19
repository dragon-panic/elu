use std::collections::{BTreeMap, HashMap};
use std::future::Future;

use elu_manifest::types::{Manifest, PackageRef};
use elu_store::hash::ManifestHash;
use semver::Version;
use url::Url;

use crate::error::ResolverError;

/// What the resolver needs from the outside world to look up package versions
/// and manifests. Implementations: a registry-backed source, an offline source
/// that consults only the local store, and a test mock.
pub trait VersionSource {
    /// List versions known for `(namespace, name)`. May be empty.
    fn list_versions(
        &self,
        package: &PackageRef,
    ) -> impl Future<Output = Result<Vec<Version>, ResolverError>>;

    /// Resolve `(package, version)` to its manifest hash + manifest body.
    /// Returns the URL the manifest can be fetched from (if any).
    fn fetch_manifest(
        &self,
        package: &PackageRef,
        version: &Version,
    ) -> impl Future<Output = Result<FetchedManifest, ResolverError>>;

    /// Resolve a hash reference directly to a manifest.
    fn fetch_by_hash(
        &self,
        hash: &ManifestHash,
    ) -> impl Future<Output = Result<FetchedManifest, ResolverError>>;
}

/// One resolved manifest, with the URL the bytes came from (if known).
#[derive(Clone, Debug)]
pub struct FetchedManifest {
    pub hash: ManifestHash,
    pub manifest: Manifest,
    pub manifest_url: Option<Url>,
    /// Per-layer download URLs, keyed by the layer's diff_id (as string).
    pub layer_urls: BTreeMap<String, Url>,
}

/// Offline source: serves only what is already in the caller-provided
/// in-memory map of known refs and manifests. Anything not listed is an error.
pub struct OfflineSource {
    pub manifests: HashMap<ManifestHash, Manifest>,
    /// `(package, version)` → hash
    pub refs: HashMap<(PackageRef, Version), ManifestHash>,
}

impl OfflineSource {
    pub fn new() -> Self {
        Self {
            manifests: HashMap::new(),
            refs: HashMap::new(),
        }
    }

    pub fn insert(&mut self, manifest: Manifest, hash: ManifestHash) {
        let pkg: PackageRef = format!("{}/{}", manifest.package.namespace, manifest.package.name)
            .parse()
            .expect("manifest namespace/name produces valid PackageRef");
        let version = manifest.package.version.clone();
        self.refs.insert((pkg, version), hash.clone());
        self.manifests.insert(hash, manifest);
    }
}

impl Default for OfflineSource {
    fn default() -> Self {
        Self::new()
    }
}

impl VersionSource for OfflineSource {
    async fn list_versions(
        &self,
        package: &PackageRef,
    ) -> Result<Vec<Version>, ResolverError> {
        let mut out: Vec<Version> = self
            .refs
            .keys()
            .filter(|(p, _)| p == package)
            .map(|(_, v)| v.clone())
            .collect();
        out.sort();
        Ok(out)
    }

    async fn fetch_manifest(
        &self,
        package: &PackageRef,
        version: &Version,
    ) -> Result<FetchedManifest, ResolverError> {
        let hash = self
            .refs
            .get(&(package.clone(), version.clone()))
            .ok_or_else(|| ResolverError::NotInLocalStore {
                package: package.clone(),
                version: elu_manifest::types::VersionSpec::Range(
                    semver::VersionReq::parse(&format!("={version}")).unwrap(),
                ),
            })?
            .clone();
        let manifest = self
            .manifests
            .get(&hash)
            .ok_or_else(|| ResolverError::NotInLocalStore {
                package: package.clone(),
                version: elu_manifest::types::VersionSpec::Range(
                    semver::VersionReq::parse(&format!("={version}")).unwrap(),
                ),
            })?
            .clone();
        Ok(FetchedManifest {
            hash,
            manifest,
            manifest_url: None,
            layer_urls: BTreeMap::new(),
        })
    }

    async fn fetch_by_hash(
        &self,
        hash: &ManifestHash,
    ) -> Result<FetchedManifest, ResolverError> {
        let manifest = self
            .manifests
            .get(hash)
            .ok_or_else(|| {
                ResolverError::Source(format!("hash {hash} not in offline store"))
            })?
            .clone();
        Ok(FetchedManifest {
            hash: hash.clone(),
            manifest,
            manifest_url: None,
            layer_urls: BTreeMap::new(),
        })
    }
}
