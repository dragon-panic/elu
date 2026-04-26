mod common;

use common::{Tar, env, store_plain};

use elu_hooks::HookMode;
use elu_stacker::{StackError, stack};
use elu_manifest::types::{Hook, HookOp, Manifest, Package, PackageRef, PatchSource};
use elu_resolver::types::{FetchPlan, Resolution, ResolvedManifest};
use elu_store::hash::{DiffId, Hash, HashAlgo, ManifestHash};
use semver::Version;

fn pkg(name: &str) -> PackageRef {
    name.parse().unwrap()
}

fn manifest_with_hook(ops: Vec<HookOp>) -> Manifest {
    Manifest {
        schema: 1,
        package: Package {
            namespace: "ns".into(),
            name: "p".into(),
            version: Version::new(1, 0, 0),
            kind: "native".into(),
            description: "".into(),
            tags: vec![],
            extra: Default::default(),
        },
        layers: vec![],
        dependencies: vec![],
        hook: Hook { ops },
        metadata: Default::default(),
        extra: Default::default(),
    }
}

fn resolution(layers: Vec<DiffId>, manifest: Manifest) -> Resolution {
    Resolution {
        manifests: vec![ResolvedManifest {
            package: pkg(&format!("{}/{}", manifest.package.namespace, manifest.package.name)),
            hash: ManifestHash(Hash::new(HashAlgo::Sha256, [0xff; 32])),
            manifest,
        }],
        layers,
        fetch_plan: FetchPlan::default(),
    }
}

#[test]
fn hook_mode_off_skips_hook_and_succeeds() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("base.txt", b"layer", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);

    // Hook would fail (patch on missing file). HookMode::Off must skip it.
    let manifest = manifest_with_hook(vec![HookOp::Patch {
        file: "missing.txt".into(),
        source: PatchSource::Inline {
            diff: "noop".into(),
        },
        fuzz: false,
    }]);
    let res = resolution(vec![did], manifest);

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    let stats = stack(&e.store, &res, &target, HookMode::Off, false).unwrap();

    assert_eq!(stats.hook.ops_run, 0);
    assert!(target.join("base.txt").as_std_path().exists());
}

#[test]
fn pre_existing_target_fails_without_force() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("a.txt", b"new", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);
    let res = resolution(vec![did], manifest_with_hook(vec![]));

    // Pre-create the target with content the user might want preserved.
    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    std::fs::create_dir_all(target.as_std_path()).unwrap();
    std::fs::write(target.join("preexisting.txt").as_std_path(), b"keep me").unwrap();

    let err = stack(&e.store, &res, &target, HookMode::Safe, false).unwrap_err();
    assert!(matches!(err, StackError::TargetExists(_)));
    // Existing content untouched.
    assert_eq!(common::read_to_string(&target.join("preexisting.txt")), "keep me");
    assert!(!target.join("a.txt").as_std_path().exists());
}

#[test]
fn pre_existing_target_replaced_with_force() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("a.txt", b"new", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);
    let res = resolution(vec![did], manifest_with_hook(vec![]));

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    std::fs::create_dir_all(target.as_std_path()).unwrap();
    std::fs::write(target.join("preexisting.txt").as_std_path(), b"keep me").unwrap();

    stack(&e.store, &res, &target, HookMode::Safe, true).unwrap();

    // Old content gone, new layer materialized.
    assert!(!target.join("preexisting.txt").as_std_path().exists());
    assert_eq!(common::read_to_string(&target.join("a.txt")), "new");
}
