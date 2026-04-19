mod common;

use common::{InMemorySource, make_manifest, pkgref, vrange};
use elu_resolver::error::ResolverError;
use elu_resolver::lockfile::{Lockfile, LockfileEntry};
use elu_resolver::resolve;
use elu_resolver::types::RootRef;
use semver::Version;

/// Slice 5: when the lockfile pin does not satisfy the constraint, the
/// resolver returns LockMismatch — never silently upgrades.
#[tokio::test]
async fn lockfile_mismatch_returns_lock_mismatch() {
    let mut src = InMemorySource::new();
    src.add(make_manifest("acme", "thing", "1.2.3", vec![]));
    src.add(make_manifest("acme", "thing", "2.5.0", vec![]));

    let pkg = pkgref("acme", "thing");
    let stale_hash = elu_manifest::manifest_hash(&make_manifest("acme", "thing", "1.2.3", vec![]));

    let lockfile = Lockfile {
        schema: 1,
        packages: vec![LockfileEntry {
            namespace: "acme".into(),
            name: "thing".into(),
            version: Version::parse("1.2.3").unwrap(),
            hash: stale_hash,
        }],
    };

    let root = RootRef {
        package: pkg.clone(),
        version: vrange("^2.0"),
    };

    let err = resolve(&[root], &src, Some(&lockfile), None)
        .await
        .expect_err("resolve should fail");

    match err {
        ResolverError::LockMismatch {
            package,
            pinned,
            spec,
        } => {
            assert_eq!(package, pkg);
            assert_eq!(pinned, Version::parse("1.2.3").unwrap());
            assert!(spec.contains("^2"), "spec rendering: {spec}");
        }
        other => panic!("expected LockMismatch, got {other:?}"),
    }
    assert_eq!(
        src.list_call_count(&pkg),
        0,
        "mismatch path must not consult registry either"
    );
}
