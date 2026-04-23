use elu_layers::flatten;
use elu_manifest::types::{Manifest, Package, PackageRef};
use elu_resolver::types::{FetchPlan, Resolution, ResolvedManifest};
use elu_store::hash::{DiffId, Hash, HashAlgo, ManifestHash};
use semver::Version;

fn diff(byte: u8) -> DiffId {
    DiffId(Hash::new(HashAlgo::Sha256, [byte; 32]))
}

fn manifest_hash(byte: u8) -> ManifestHash {
    ManifestHash(Hash::new(HashAlgo::Sha256, [byte; 32]))
}

fn pkg(name: &str) -> PackageRef {
    name.parse().unwrap()
}

fn empty_manifest(ns: &str, name: &str) -> Manifest {
    Manifest {
        schema: 1,
        package: Package {
            namespace: ns.into(),
            name: name.into(),
            version: Version::new(1, 0, 0),
            kind: "native".into(),
            description: "test".into(),
            tags: vec![],
        },
        layers: vec![],
        dependencies: vec![],
        hook: Default::default(),
        metadata: Default::default(),
    }
}

#[test]
fn flatten_returns_resolutions_layers_verbatim() {
    let layers = vec![diff(0x01), diff(0x02), diff(0x03)];
    let resolution = Resolution {
        manifests: vec![ResolvedManifest {
            package: pkg("ns/a"),
            hash: manifest_hash(0xa1),
            manifest: empty_manifest("ns", "a"),
        }],
        layers: layers.clone(),
        fetch_plan: FetchPlan::default(),
    };
    let out = flatten(&resolution);
    assert_eq!(out, layers.as_slice());
}

#[test]
fn flatten_preserves_dedup_and_dfs_order() {
    // The resolver guarantees dedup + DFS order. flatten is a thin accessor;
    // this test pins the contract.
    let resolution = Resolution {
        manifests: vec![],
        layers: vec![diff(0x10), diff(0x20), diff(0x30)],
        fetch_plan: FetchPlan::default(),
    };
    let out = flatten(&resolution);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0], diff(0x10));
    assert_eq!(out[1], diff(0x20));
    assert_eq!(out[2], diff(0x30));
}
