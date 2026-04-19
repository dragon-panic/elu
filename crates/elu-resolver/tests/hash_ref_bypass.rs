mod common;

use common::{InMemorySource, make_manifest, pkgref};
use elu_manifest::types::VersionSpec;
use elu_resolver::resolve;
use elu_resolver::types::RootRef;

/// Slice 1: a root ref pinned to a manifest hash resolves to that hash
/// without consulting `list_versions`.
#[tokio::test]
async fn hash_ref_resolves_directly_without_listing_versions() {
    let mut src = InMemorySource::new();
    let m = make_manifest("acme", "thing", "1.2.3", vec![]);
    let hash = src.add(m);

    let pkg = pkgref("acme", "thing");
    let root = RootRef {
        package: pkg.clone(),
        version: VersionSpec::Pinned(hash.clone()),
    };

    let resolution = resolve(&[root], &src, None, None).await.expect("resolve ok");

    assert_eq!(resolution.manifests.len(), 1);
    assert_eq!(resolution.manifests[0].hash, hash);
    assert_eq!(resolution.manifests[0].package, pkg);
    assert_eq!(
        src.list_call_count(&pkg),
        0,
        "hash refs must bypass version listing"
    );
}
