use elu_author::explain::{diff_manifests, explain_text, ExplainDiff};
use elu_manifest::{Dependency, Hook, HookOp, Layer, Manifest, Metadata, Package, PackageRef, VersionSpec};
use elu_store::hash::{Hash, HashAlgo};
use semver::Version;

fn hash_of(n: u8) -> Hash {
    Hash::new(HashAlgo::Sha256, [n; 32])
}

fn manifest_a() -> Manifest {
    Manifest {
        schema: 1,
        package: Package {
            namespace: "dragon".into(),
            name: "tree".into(),
            version: Version::parse("1.0.0").unwrap(),
            kind: "native".into(),
            description: "Prints a tree".into(),
            tags: vec!["example".into()],
            extra: Default::default(),
        },
        layers: vec![Layer {
            diff_id: Some(elu_store::hash::DiffId(hash_of(1))),
            size: Some(1024),
            name: Some("bin".into()),
            include: vec![],
            exclude: vec![],
            strip: None,
            place: None,
            mode: None,
            follow_symlinks: false,
            extra: Default::default(),
        }],
        dependencies: vec![Dependency {
            reference: "ox-community/shell".parse::<PackageRef>().unwrap(),
            version: VersionSpec::Range("^1.0".parse().unwrap()),
        }],
        hook: Hook {
            ops: vec![HookOp::Chmod {
                paths: vec!["bin/*".into()],
                mode: "+x".into(),
            }],
        },
        metadata: Metadata::default(),
        extra: Default::default(),
    }
}

#[test]
fn explain_text_lists_package_layers_and_deps() {
    let text = explain_text(&manifest_a());
    assert!(text.contains("dragon/tree"));
    assert!(text.contains("1.0.0"));
    assert!(text.contains("Layers"));
    assert!(text.contains("bin"));
    assert!(text.contains("Dependencies"));
    assert!(text.contains("ox-community/shell"));
    assert!(text.contains("Hook operations"));
    assert!(text.contains("chmod"));
}

#[test]
fn diff_reports_added_and_removed_deps_and_hook_ops() {
    let mut a = manifest_a();
    let mut b = manifest_a();

    // Add a dep to b
    b.dependencies.push(Dependency {
        reference: "ox-community/fs".parse::<PackageRef>().unwrap(),
        version: VersionSpec::Any,
    });

    // Remove the chmod, add a mkdir in b
    b.hook = Hook {
        ops: vec![HookOp::Mkdir {
            path: "etc/".into(),
            mode: None,
            parents: true,
        }],
    };

    // Bump version
    a.package.version = Version::parse("1.0.0").unwrap();
    b.package.version = Version::parse("1.1.0").unwrap();

    let d: ExplainDiff = diff_manifests(&a, &b);
    assert_eq!(d.version_change.as_deref(), Some("1.0.0 -> 1.1.0"));
    assert_eq!(d.dependencies_added, vec!["ox-community/fs".to_string()]);
    assert!(d.dependencies_removed.is_empty());
    assert_eq!(d.hook_ops_removed, vec!["chmod".to_string()]);
    assert_eq!(d.hook_ops_added, vec!["mkdir".to_string()]);
}
