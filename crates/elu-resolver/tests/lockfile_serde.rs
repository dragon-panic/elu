use elu_resolver::lockfile::{Lockfile, LockfileEntry};
use elu_store::hash::{Hash, HashAlgo, ManifestHash};
use semver::Version;

/// Slice 8: a lockfile parses, serializes, and round-trips against the
/// PRD example shape.
#[test]
fn lockfile_round_trips_through_toml() {
    let toml = r#"
schema = 1

[[package]]
namespace = "ox-community"
name = "postgres-query"
version = "0.3.2"
hash = "sha256:8f7a1c2e4d3b00000000000000000000000000000000000000000000000000ab"

[[package]]
namespace = "ox-community"
name = "shell"
version = "1.1.0"
hash = "sha256:3b9e0a77f100000000000000000000000000000000000000000000000000000c"
"#;

    let parsed = Lockfile::from_toml_str(toml).expect("parse ok");
    assert_eq!(parsed.schema, 1);
    assert_eq!(parsed.packages.len(), 2);
    assert_eq!(parsed.packages[0].namespace, "ox-community");
    assert_eq!(parsed.packages[0].name, "postgres-query");
    assert_eq!(parsed.packages[0].version, Version::parse("0.3.2").unwrap());

    let rendered = parsed.to_toml_string().expect("serialize ok");
    let reparsed = Lockfile::from_toml_str(&rendered).expect("reparse ok");
    assert_eq!(parsed, reparsed);
}

#[test]
fn lockfile_lookup_by_namespace_and_name() {
    let lock = Lockfile {
        schema: 1,
        packages: vec![
            LockfileEntry {
                namespace: "acme".into(),
                name: "thing".into(),
                version: Version::parse("1.0.0").unwrap(),
                hash: ManifestHash(Hash::new(HashAlgo::Sha256, [1; 32])),
            },
            LockfileEntry {
                namespace: "acme".into(),
                name: "other".into(),
                version: Version::parse("2.0.0").unwrap(),
                hash: ManifestHash(Hash::new(HashAlgo::Sha256, [2; 32])),
            },
        ],
    };
    assert!(lock.lookup("acme", "thing").is_some());
    assert!(lock.lookup("acme", "missing").is_none());
}
