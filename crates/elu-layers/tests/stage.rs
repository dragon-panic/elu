mod common;

use common::{Tar, env, store_plain, work};

use elu_hooks::HookMode;
use elu_layers::stage;
use elu_manifest::types::{Hook, Manifest, Package, PackageRef};
use elu_resolver::types::{FetchPlan, Resolution, ResolvedManifest};
use elu_store::hash::{DiffId, Hash, HashAlgo, ManifestHash};
use semver::Version;

fn pkg(name: &str) -> PackageRef {
    name.parse().unwrap()
}

fn resolution(layers: Vec<DiffId>) -> Resolution {
    let manifest = Manifest {
        schema: 1,
        package: Package {
            namespace: "ns".into(),
            name: "p".into(),
            version: Version::new(1, 0, 0),
            kind: "native".into(),
            description: "".into(),
            tags: vec![],
        },
        layers: vec![],
        dependencies: vec![],
        hook: Hook { ops: vec![] },
        metadata: Default::default(),
    };
    Resolution {
        manifests: vec![ResolvedManifest {
            package: pkg("ns/p"),
            hash: ManifestHash(Hash::new(HashAlgo::Sha256, [0xff; 32])),
            manifest,
        }],
        layers,
        fetch_plan: FetchPlan::default(),
    }
}

#[test]
fn stage_produces_populated_staging_dir() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("hello.txt", b"world", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);
    let res = resolution(vec![did]);

    let parent = work(&e);
    let (staging, stats) = stage(&e.store, &res, parent, HookMode::Safe).unwrap();
    assert_eq!(stats.layers, 1);
    assert!(staging.path().as_std_path().exists());
    assert_eq!(
        common::read_to_string(&staging.path().join("hello.txt")),
        "world"
    );
    // Staging lives under parent.
    assert_eq!(staging.path().parent().unwrap(), parent);
}

#[test]
fn staging_drop_cleans_up_when_not_disarmed() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("file.txt", b"x", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);
    let res = resolution(vec![did]);

    let parent = work(&e);
    let recorded_path = {
        let (staging, _) = stage(&e.store, &res, parent, HookMode::Safe).unwrap();
        let p = staging.path().to_path_buf();
        assert!(p.as_std_path().exists());
        p
        // staging dropped here
    };
    assert!(!recorded_path.as_std_path().exists());
}

#[test]
fn into_path_disarms_drop() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("file.txt", b"x", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);
    let res = resolution(vec![did]);

    let parent = work(&e);
    let (staging, _) = stage(&e.store, &res, parent, HookMode::Safe).unwrap();
    let detached = staging.into_path();
    assert!(detached.as_std_path().exists());
    // Clean up so TempDir::drop doesn't complain.
    std::fs::remove_dir_all(detached.as_std_path()).unwrap();
}
