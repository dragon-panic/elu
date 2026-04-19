mod common;

use common::{InMemorySource, make_manifest, pkgref, vrange};
use elu_resolver::lockfile::{Lockfile, LockfileEntry};
use elu_resolver::resolve;
use elu_resolver::types::RootRef;
use semver::Version;

/// Slice 4: when the lockfile pins a version that satisfies the constraint,
/// the resolver returns the lock entry without listing versions from the source.
#[tokio::test]
async fn lockfile_pin_short_circuits_version_listing() {
    let mut src = InMemorySource::new();
    src.add(make_manifest("acme", "thing", "1.0.0", vec![]));
    src.add(make_manifest("acme", "thing", "1.2.3", vec![]));
    src.add(make_manifest("acme", "thing", "1.5.0", vec![]));
    src.add(make_manifest("acme", "thing", "2.0.0", vec![]));

    let pkg = pkgref("acme", "thing");
    let pinned_hash = elu_manifest::manifest_hash(&make_manifest("acme", "thing", "1.2.3", vec![]));

    let lockfile = Lockfile {
        schema: 1,
        packages: vec![LockfileEntry {
            namespace: "acme".into(),
            name: "thing".into(),
            version: Version::parse("1.2.3").unwrap(),
            hash: pinned_hash.clone(),
        }],
    };

    let root = RootRef {
        package: pkg.clone(),
        version: vrange("^1.0"),
    };

    let resolution = resolve(&[root], &src, Some(&lockfile), None)
        .await
        .expect("resolve ok");

    assert_eq!(resolution.manifests.len(), 1);
    assert_eq!(resolution.manifests[0].hash, pinned_hash);
    assert_eq!(
        resolution.manifests[0].manifest.package.version.to_string(),
        "1.2.3"
    );
    assert_eq!(
        src.list_call_count(&pkg),
        0,
        "satisfying lock pin must skip version listing"
    );
}
