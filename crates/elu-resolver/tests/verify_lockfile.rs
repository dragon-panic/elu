mod common;

use common::{dep, make_manifest, vrange};
use elu_resolver::error::ResolverError;
use elu_resolver::lockfile::{Lockfile, LockfileEntry};
use elu_resolver::verify;
use elu_store::hash::{Hash, HashAlgo, ManifestHash};
use semver::Version;

fn synth(byte: u8) -> ManifestHash {
    ManifestHash(Hash::new(HashAlgo::Sha256, [byte; 32]))
}

/// Slice 9: verify() succeeds when the lockfile satisfies every dep, and
/// returns a missing-pin error when a dep has no entry.
#[test]
fn verify_accepts_matching_lockfile() {
    let manifest = make_manifest(
        "acme",
        "root",
        "0.1.0",
        vec![
            dep("acme", "thing", vrange("^1.0")),
            dep("acme", "other", vrange("^2.0")),
        ],
    );
    let lock = Lockfile {
        schema: 1,
        packages: vec![
            LockfileEntry {
                namespace: "acme".into(),
                name: "thing".into(),
                version: Version::parse("1.2.3").unwrap(),
                hash: synth(0xaa),
            },
            LockfileEntry {
                namespace: "acme".into(),
                name: "other".into(),
                version: Version::parse("2.5.0").unwrap(),
                hash: synth(0xbb),
            },
        ],
    };

    verify(&manifest, &lock).expect("matching lock verifies");
}

#[test]
fn verify_reports_missing_pin_for_new_dep() {
    let manifest = make_manifest(
        "acme",
        "root",
        "0.1.0",
        vec![
            dep("acme", "thing", vrange("^1.0")),
            dep("acme", "newdep", vrange("^1.0")), // missing from lock
        ],
    );
    let lock = Lockfile {
        schema: 1,
        packages: vec![LockfileEntry {
            namespace: "acme".into(),
            name: "thing".into(),
            version: Version::parse("1.2.3").unwrap(),
            hash: synth(0xaa),
        }],
    };

    let errs = verify(&manifest, &lock).expect_err("expected missing pin");
    assert_eq!(errs.len(), 1);
    match &errs[0] {
        ResolverError::NoMatch { package, .. } => {
            assert_eq!(package.as_str(), "acme/newdep");
        }
        other => panic!("expected NoMatch for missing dep, got {other:?}"),
    }
}
