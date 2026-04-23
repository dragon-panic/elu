mod common;

use common::{Tar, env, store_plain};

use elu_hooks::HookMode;
use elu_layers::stack;
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
        },
        layers: vec![],
        dependencies: vec![],
        hook: Hook { ops },
        metadata: Default::default(),
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
fn failed_hook_rolls_back_target_untouched() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("base.txt", b"layer", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);

    // A patch op against a missing file fails.
    let manifest = manifest_with_hook(vec![HookOp::Patch {
        file: "does-not-exist.txt".into(),
        source: PatchSource::Inline {
            diff: "--- a\n+++ b\n@@ -1 +1 @@\n-x\n+y\n".into(),
        },
        fuzz: false,
    }]);
    let res = resolution(vec![did], manifest);

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    let result = stack(&e.store, &res, &target, HookMode::Safe, false);
    assert!(result.is_err(), "expected error, got {result:?}");

    // Target was never created.
    assert!(!target.as_std_path().exists());

    // No staging dir leaked next to target.
    let parent = target.parent().unwrap();
    let leftover: Vec<_> = std::fs::read_dir(parent.as_std_path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(leftover.is_empty(), "parent dir not cleaned: {leftover:?}");
}

#[test]
fn failed_layer_apply_rolls_back() {
    let e = env();
    // Put a layer with an unsafe path into the store; apply rejects it.
    let bad_tar = Tar::new()
        .raw_path("../escape", b"x")
        .into_bytes();
    let did = store_plain(&e, &bad_tar);
    let res = resolution(vec![did], manifest_with_hook(vec![]));

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    let result = stack(&e.store, &res, &target, HookMode::Safe, false);
    assert!(result.is_err());
    assert!(!target.as_std_path().exists());
}
