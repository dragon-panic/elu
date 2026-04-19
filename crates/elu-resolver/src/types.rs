use elu_manifest::types::{Manifest, PackageRef, VersionSpec};
use elu_store::hash::{DiffId, ManifestHash};
use url::Url;

/// A reference handed to `resolve()` as a starting point.
#[derive(Clone, Debug, PartialEq)]
pub struct RootRef {
    pub package: PackageRef,
    pub version: VersionSpec,
}

/// One manifest in a successful resolution.
#[derive(Clone, Debug)]
pub struct ResolvedManifest {
    pub package: PackageRef,
    pub hash: ManifestHash,
    pub manifest: Manifest,
}

/// Output of `resolve()`.
#[derive(Clone, Debug)]
pub struct Resolution {
    pub manifests: Vec<ResolvedManifest>,
    pub layers: Vec<DiffId>,
    pub fetch_plan: FetchPlan,
}

/// Blobs the resolver determined are not in the local store.
#[derive(Clone, Debug, Default)]
pub struct FetchPlan {
    pub items: Vec<FetchItem>,
}

#[derive(Clone, Debug)]
pub struct FetchItem {
    pub kind: FetchKind,
    pub url: Option<Url>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchKind {
    Manifest(ManifestHash),
    Layer(DiffId),
}
