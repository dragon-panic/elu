mod common;

use common::{InMemorySource, make_manifest, pkgref};
use elu_manifest::types::VersionSpec;
use elu_resolver::resolve;
use elu_resolver::types::RootRef;

/// Slice 2: a single name-only root resolves to the highest available
/// version among the source's listings.
#[tokio::test]
async fn name_only_root_picks_highest_version() {
    let mut src = InMemorySource::new();
    src.add(make_manifest("acme", "thing", "1.0.0", vec![]));
    src.add(make_manifest("acme", "thing", "1.1.0", vec![]));
    let want_hash = elu_manifest::manifest_hash(&make_manifest("acme", "thing", "2.0.0", vec![]));
    src.add(make_manifest("acme", "thing", "2.0.0", vec![]));

    let pkg = pkgref("acme", "thing");
    let root = RootRef {
        package: pkg.clone(),
        version: VersionSpec::Any,
    };

    let resolution = resolve(&[root], &src, None, None).await.expect("resolve ok");

    assert_eq!(resolution.manifests.len(), 1);
    assert_eq!(resolution.manifests[0].hash, want_hash);
    assert_eq!(resolution.manifests[0].manifest.package.version.to_string(), "2.0.0");
}
