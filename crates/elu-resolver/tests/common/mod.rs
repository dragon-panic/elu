#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;

use elu_manifest::types::{
    Dependency, Layer, Manifest, Metadata, Package, PackageRef, VersionSpec,
};
use elu_resolver::error::ResolverError;
use elu_resolver::source::{FetchedManifest, VersionSource};
use elu_store::hash::{DiffId, Hash, HashAlgo, ManifestHash};
use semver::Version;

/// In-memory `VersionSource` for tests. Tracks call counts so tests can
/// assert that, e.g., the registry was *not* consulted for a locked dep.
pub struct InMemorySource {
    pub manifests: HashMap<ManifestHash, Manifest>,
    /// `(package, version)` → hash
    pub refs: HashMap<(PackageRef, Version), ManifestHash>,
    pub list_calls: RefCell<HashMap<PackageRef, usize>>,
}

impl InMemorySource {
    pub fn new() -> Self {
        Self {
            manifests: HashMap::new(),
            refs: HashMap::new(),
            list_calls: RefCell::new(HashMap::new()),
        }
    }

    pub fn add(&mut self, manifest: Manifest) -> ManifestHash {
        let hash = elu_manifest::manifest_hash(&manifest);
        let pkg: PackageRef = pkgref(&manifest.package.namespace, &manifest.package.name);
        let version = manifest.package.version.clone();
        self.refs.insert((pkg, version), hash.clone());
        self.manifests.insert(hash.clone(), manifest);
        hash
    }

    pub fn list_call_count(&self, package: &PackageRef) -> usize {
        self.list_calls.borrow().get(package).copied().unwrap_or(0)
    }
}

impl VersionSource for InMemorySource {
    async fn list_versions(
        &self,
        package: &PackageRef,
    ) -> Result<Vec<Version>, ResolverError> {
        *self.list_calls.borrow_mut().entry(package.clone()).or_insert(0) += 1;
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
            .cloned()
            .ok_or_else(|| {
                ResolverError::Source(format!("no manifest for {package}@{version}"))
            })?;
        let manifest = self.manifests.get(&hash).cloned().unwrap();
        Ok(FetchedManifest {
            hash,
            manifest,
            manifest_url: None,
            layer_urls: Default::default(),
        })
    }

    async fn fetch_by_hash(
        &self,
        hash: &ManifestHash,
    ) -> Result<FetchedManifest, ResolverError> {
        let manifest = self
            .manifests
            .get(hash)
            .cloned()
            .ok_or_else(|| ResolverError::Source(format!("no manifest for {hash}")))?;
        Ok(FetchedManifest {
            hash: hash.clone(),
            manifest,
            manifest_url: None,
            layer_urls: Default::default(),
        })
    }
}

pub fn pkgref(ns: &str, name: &str) -> PackageRef {
    format!("{ns}/{name}").parse().unwrap()
}

pub fn make_manifest(
    namespace: &str,
    name: &str,
    version: &str,
    deps: Vec<Dependency>,
) -> Manifest {
    Manifest {
        schema: 1,
        package: Package {
            namespace: namespace.into(),
            name: name.into(),
            version: Version::parse(version).unwrap(),
            kind: "lib".into(),
            description: format!("{namespace}/{name} test"),
            tags: vec![],
        },
        layers: vec![],
        dependencies: deps,
        hook: Default::default(),
        metadata: Metadata::default(),
    }
}

pub fn dep(ns: &str, name: &str, spec: VersionSpec) -> Dependency {
    Dependency {
        reference: pkgref(ns, name),
        version: spec,
    }
}

pub fn vrange(s: &str) -> VersionSpec {
    VersionSpec::Range(semver::VersionReq::parse(s).unwrap())
}

pub fn synth_hash(byte: u8) -> ManifestHash {
    ManifestHash(Hash::new(HashAlgo::Sha256, [byte; 32]))
}

pub fn synth_diff(byte: u8) -> DiffId {
    DiffId(Hash::new(HashAlgo::Sha256, [byte; 32]))
}

pub fn make_manifest_with_layers(
    namespace: &str,
    name: &str,
    version: &str,
    deps: Vec<Dependency>,
    layer_diffs: Vec<DiffId>,
) -> Manifest {
    let mut m = make_manifest(namespace, name, version, deps);
    m.layers = layer_diffs
        .into_iter()
        .map(|d| Layer {
            diff_id: Some(d),
            size: Some(0),
            name: None,
            include: vec![],
            exclude: vec![],
            strip: None,
            place: None,
            mode: None,
            follow_symlinks: false,
        })
        .collect();
    m
}
