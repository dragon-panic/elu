use std::fs;

use camino::Utf8Path;
use elu_author::build::{build, BuildOpts};
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::store::Store;

const SIMPLE_TOML: &str = r#"
schema = 1

[package]
namespace   = "dragon"
name        = "hello-tree"
version     = "0.1.0"
kind        = "native"
description = "A tiny tree"

[[layer]]
name    = "bin"
include = ["target/release/hello"]
strip   = "target/release/"
place   = "bin/"

[[hook.op]]
type  = "chmod"
paths = ["bin/*"]
mode  = "+x"
"#;

fn scaffold(root: &Utf8Path) {
    fs::write(root.join("elu.toml"), SIMPLE_TOML).unwrap();
    fs::create_dir_all(root.join("target/release")).unwrap();
    fs::write(root.join("target/release/hello"), b"#!bin\n").unwrap();
}

#[test]
fn build_writes_manifest_and_returns_hash() {
    let proj = tempfile::tempdir().unwrap();
    let proj_root = Utf8Path::from_path(proj.path()).unwrap();
    scaffold(proj_root);

    let store_dir = tempfile::tempdir().unwrap();
    let store_root = Utf8Path::from_path(store_dir.path()).unwrap();
    let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();

    let (report, artifact) =
        build(proj_root, &store, &BuildOpts::default()).expect("build ok");
    assert!(report.ok, "report errors = {:?}", report.errors);
    let artifact = artifact.expect("hash returned");

    let raw = store
        .get_manifest(&artifact.manifest_hash)
        .unwrap()
        .expect("manifest in store");
    let parsed: elu_manifest::Manifest = serde_json::from_slice(&raw).unwrap();
    elu_manifest::validate::validate_stored(&parsed).expect("stored form");
    assert_eq!(parsed.layers.len(), 1);
    let layer = &parsed.layers[0];
    assert!(layer.diff_id.is_some());
    assert!(layer.size.is_some());
    assert!(layer.include.is_empty());
    assert!(layer.strip.is_none());

    // Layer blob retrievable by diff_id
    let blob_id = store
        .resolve_diff(layer.diff_id.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert!(store.has(&blob_id).unwrap());
}

#[test]
fn build_deterministic_across_clean_stores() {
    fn do_build() -> elu_store::hash::ManifestHash {
        let proj = tempfile::tempdir().unwrap();
        let proj_root = Utf8Path::from_path(proj.path()).unwrap();
        scaffold(proj_root);
        let store_dir = tempfile::tempdir().unwrap();
        let store_root = Utf8Path::from_path(store_dir.path()).unwrap();
        let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();
        build(proj_root, &store, &BuildOpts::default())
            .unwrap()
            .1
            .unwrap()
            .manifest_hash
    }
    let a = do_build();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let b = do_build();
    assert_eq!(a.to_string(), b.to_string());
}

#[test]
fn build_reports_layer_include_no_matches() {
    let proj = tempfile::tempdir().unwrap();
    let proj_root = Utf8Path::from_path(proj.path()).unwrap();
    fs::write(
        proj_root.join("elu.toml"),
        r#"
schema = 1
[package]
namespace   = "x"
name        = "y"
version     = "0.1.0"
kind        = "native"
description = "z"

[[layer]]
name    = "bin"
include = ["target/release/no-such"]
"#,
    )
    .unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store_root = Utf8Path::from_path(store_dir.path()).unwrap();
    let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();

    let (report, artifact) = build(proj_root, &store, &BuildOpts::default()).unwrap();
    assert!(!report.ok);
    assert!(artifact.is_none());
    assert!(
        report
            .errors
            .iter()
            .any(|d| d.code == elu_author::report::ErrorCode::LayerIncludeNoMatches)
    );
}
