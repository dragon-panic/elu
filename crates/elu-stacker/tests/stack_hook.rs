mod common;

use common::{Tar, env, store_plain};

use elu_hooks::HookMode;
use elu_stacker::stack;
use elu_manifest::types::{Hook, HookOp, Manifest, Package, PackageRef};
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
fn hook_runs_after_layers_and_before_finalize() {
    let e = env();
    let layer = Tar::new()
        .file_mode_owned("base.txt", b"layer", 0o644)
        .into_bytes();
    let did = store_plain(&e, &layer);

    let manifest = manifest_with_hook(vec![HookOp::Write {
        path: "from-hook.txt".into(),
        content: "hook-content".into(),
        mode: None,
        replace: false,
    }]);
    let res = resolution(vec![did], manifest);

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    let stats = stack(&e.store, &res, &target, HookMode::Safe, false).unwrap();

    assert_eq!(stats.hook.ops_run, 1);
    assert_eq!(common::read_to_string(&target.join("base.txt")), "layer");
    assert_eq!(common::read_to_string(&target.join("from-hook.txt")), "hook-content");
}

#[test]
fn hook_can_observe_merged_layer_state() {
    // The hook sees the fully-merged stack (per PRD: hook runs after the
    // full stack is assembled in staging).
    let e = env();
    let l1 = Tar::new()
        .file_mode_owned("a.txt", b"first", 0o644)
        .into_bytes();
    let l2 = Tar::new()
        .file_mode_owned("a.txt", b"second", 0o644)
        .into_bytes();

    // Hook deletes a.txt — it must see the merged state where a.txt = "second".
    let manifest = manifest_with_hook(vec![HookOp::Delete {
        paths: vec!["a.txt".into()],
    }]);
    let res = resolution(
        vec![store_plain(&e, &l1), store_plain(&e, &l2)],
        manifest,
    );

    let target = camino::Utf8PathBuf::from_path_buf(e.work_dir.path().join("out")).unwrap();
    stack(&e.store, &res, &target, HookMode::Safe, false).unwrap();

    assert!(!target.join("a.txt").as_std_path().exists());
}
