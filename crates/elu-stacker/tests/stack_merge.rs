mod common;

use common::{Tar, env, store_plain};

use elu_hooks::HookMode;
use elu_stacker::stack;
use elu_manifest::types::{Manifest, Package, PackageRef};
use elu_resolver::types::{FetchPlan, Resolution, ResolvedManifest};
use elu_store::hash::{DiffId, ManifestHash};
use semver::Version;

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
            description: "".into(),
            tags: vec![],
            extra: Default::default(),
        },
        layers: vec![],
        dependencies: vec![],
        hook: Default::default(),
        metadata: Default::default(),
        extra: Default::default(),
    }
}

fn resolution(layers: Vec<DiffId>, manifest: Manifest) -> Resolution {
    Resolution {
        manifests: vec![ResolvedManifest {
            package: pkg(&format!("{}/{}", manifest.package.namespace, manifest.package.name)),
            hash: ManifestHash(elu_store::hash::Hash::new(
                elu_store::hash::HashAlgo::Sha256,
                [0xff; 32],
            )),
            manifest,
        }],
        layers,
        fetch_plan: FetchPlan::default(),
    }
}

#[test]
fn two_layers_merge_with_later_winning() {
    let e = env();
    // Layer A: a.txt = "v1", common.txt = "from-a".
    let a = Tar::new()
        .file_mode_owned("a.txt", b"v1", 0o644)
        .file_mode_owned("common.txt", b"from-a", 0o644)
        .into_bytes();
    let did_a = store_plain(&e, &a);

    // Layer B: b.txt = "v1", common.txt = "from-b" (overrides).
    let b = Tar::new()
        .file_mode_owned("b.txt", b"v1", 0o644)
        .file_mode_owned("common.txt", b"from-b", 0o644)
        .into_bytes();
    let did_b = store_plain(&e, &b);

    let res = resolution(vec![did_a, did_b], empty_manifest("ns", "p"));

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    let stats = stack(&e.store, &res, &target, HookMode::Safe, false).unwrap();

    assert_eq!(stats.layers, 2);
    assert!(target.as_std_path().is_dir());
    assert_eq!(common::read_to_string(&target.join("a.txt")), "v1");
    assert_eq!(common::read_to_string(&target.join("b.txt")), "v1");
    // Later wins.
    assert_eq!(common::read_to_string(&target.join("common.txt")), "from-b");
}

#[test]
fn target_does_not_exist_before_stack() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("only.txt", b"x", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);
    let res = resolution(vec![did], empty_manifest("ns", "p"));

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    assert!(!target.as_std_path().exists());

    stack(&e.store, &res, &target, HookMode::Safe, false).unwrap();

    assert!(target.as_std_path().is_dir());
    assert_eq!(common::read_to_string(&target.join("only.txt")), "x");
}

#[test]
fn whiteout_in_later_layer_removes_earlier_file() {
    let e = env();
    let l1 = Tar::new()
        .file_mode_owned("foo", b"v1", 0o644)
        .file_mode_owned("bar", b"v1", 0o644)
        .into_bytes();
    let l2 = Tar::new()
        .file_mode_owned(".wh.foo", b"", 0o644)
        .into_bytes();

    let res = resolution(
        vec![store_plain(&e, &l1), store_plain(&e, &l2)],
        empty_manifest("ns", "p"),
    );
    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    stack(&e.store, &res, &target, HookMode::Safe, false).unwrap();

    assert!(!target.join("foo").as_std_path().exists());
    assert!(target.join("bar").as_std_path().exists());
}
