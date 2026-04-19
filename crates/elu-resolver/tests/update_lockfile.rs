mod common;

use common::{InMemorySource, dep, make_manifest, vrange};
use elu_resolver::error::ResolverError;
use elu_resolver::lockfile::{Lockfile, LockfileEntry, update};
use semver::Version;

/// Slice 10: `update(names=["a"])` re-resolves `a` and its transitive deps;
/// unrelated entries stay pinned at their existing lock version.
#[tokio::test]
async fn update_named_target_only_unrelated_entries_untouched() {
    let mut src = InMemorySource::new();
    src.add(make_manifest("acme", "a", "1.0.0", vec![]));
    src.add(make_manifest("acme", "a", "1.5.0", vec![]));
    src.add(make_manifest("acme", "other", "2.0.0", vec![]));
    src.add(make_manifest("acme", "other", "2.7.0", vec![]));

    let manifest = make_manifest(
        "acme",
        "root",
        "0.1.0",
        vec![
            dep("acme", "a", vrange("^1")),
            dep("acme", "other", vrange("^2")),
        ],
    );

    let original_other_hash = elu_manifest::manifest_hash(&make_manifest(
        "acme", "other", "2.0.0", vec![],
    ));
    let stale_a_hash = elu_manifest::manifest_hash(&make_manifest("acme", "a", "1.0.0", vec![]));
    let new_a_hash = elu_manifest::manifest_hash(&make_manifest("acme", "a", "1.5.0", vec![]));

    let lockfile = Lockfile {
        schema: 1,
        packages: vec![
            LockfileEntry {
                namespace: "acme".into(),
                name: "a".into(),
                version: Version::parse("1.0.0").unwrap(),
                hash: stale_a_hash,
            },
            LockfileEntry {
                namespace: "acme".into(),
                name: "other".into(),
                version: Version::parse("2.0.0").unwrap(),
                hash: original_other_hash.clone(),
            },
        ],
    };

    let updated = update(&manifest, &lockfile, Some(&["a".to_string()]), &src)
        .await
        .expect("update ok");

    let a_entry = updated.lookup("acme", "a").expect("a present");
    assert_eq!(a_entry.version.to_string(), "1.5.0", "a moved forward");
    assert_eq!(a_entry.hash, new_a_hash);

    let other_entry = updated.lookup("acme", "other").expect("other present");
    assert_eq!(
        other_entry.version.to_string(),
        "2.0.0",
        "other stays pinned to existing lock"
    );
    assert_eq!(other_entry.hash, original_other_hash);
}

#[tokio::test]
async fn update_unknown_name_errors() {
    let src = InMemorySource::new();
    let manifest = make_manifest("acme", "root", "0.1.0", vec![]);
    let lock = Lockfile { schema: 1, packages: vec![] };

    let err = update(&manifest, &lock, Some(&["nope".to_string()]), &src)
        .await
        .expect_err("unknown target should error");
    assert!(matches!(err, ResolverError::UnknownUpdateTarget { .. }));
}
