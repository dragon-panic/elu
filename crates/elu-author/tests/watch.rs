use std::fs;

use camino::Utf8Path;
use elu_author::watch::{incremental_build, LayerFingerprints};
use elu_manifest::from_toml_str;
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;

const TWO_LAYER_TOML: &str = r#"
schema = 1

[package]
namespace   = "dragon"
name        = "two"
version     = "0.1.0"
kind        = "native"
description = "two layers"

[[layer]]
name    = "a"
include = ["a/*"]

[[layer]]
name    = "b"
include = ["b/*"]
"#;

fn scaffold(root: &Utf8Path) {
    fs::write(root.join("elu.toml"), TWO_LAYER_TOML).unwrap();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/file"), b"A").unwrap();
    fs::write(root.join("b/file"), b"B").unwrap();
}

#[test]
fn first_pass_packs_everything_and_records_fingerprints() {
    let proj = tempfile::tempdir().unwrap();
    let proj_root = Utf8Path::from_path(proj.path()).unwrap();
    scaffold(proj_root);
    let store_dir = tempfile::tempdir().unwrap();
    let store = FsStore::init_with_fsync(
        Utf8Path::from_path(store_dir.path()).unwrap(),
        FsyncMode::Never,
    )
    .unwrap();

    let manifest = from_toml_str(&fs::read_to_string(proj_root.join("elu.toml")).unwrap()).unwrap();
    let mut fps = LayerFingerprints::default();
    let changed = incremental_build(proj_root, &manifest, &store, &mut fps).unwrap();

    assert_eq!(changed, vec![0usize, 1]);
    assert_eq!(fps.len(), 2);
}

#[test]
fn no_changes_means_no_rebuilds() {
    let proj = tempfile::tempdir().unwrap();
    let proj_root = Utf8Path::from_path(proj.path()).unwrap();
    scaffold(proj_root);
    let store_dir = tempfile::tempdir().unwrap();
    let store = FsStore::init_with_fsync(
        Utf8Path::from_path(store_dir.path()).unwrap(),
        FsyncMode::Never,
    )
    .unwrap();

    let manifest = from_toml_str(&fs::read_to_string(proj_root.join("elu.toml")).unwrap()).unwrap();
    let mut fps = LayerFingerprints::default();
    let _ = incremental_build(proj_root, &manifest, &store, &mut fps).unwrap();
    let changed = incremental_build(proj_root, &manifest, &store, &mut fps).unwrap();
    assert!(changed.is_empty(), "second pass should see no changes");
}

#[test]
fn only_changed_layer_repacks() {
    let proj = tempfile::tempdir().unwrap();
    let proj_root = Utf8Path::from_path(proj.path()).unwrap();
    scaffold(proj_root);
    let store_dir = tempfile::tempdir().unwrap();
    let store = FsStore::init_with_fsync(
        Utf8Path::from_path(store_dir.path()).unwrap(),
        FsyncMode::Never,
    )
    .unwrap();

    let manifest = from_toml_str(&fs::read_to_string(proj_root.join("elu.toml")).unwrap()).unwrap();
    let mut fps = LayerFingerprints::default();
    let _ = incremental_build(proj_root, &manifest, &store, &mut fps).unwrap();

    // Ensure mtime delta
    std::thread::sleep(std::time::Duration::from_millis(10));
    // Mutate layer `a` only.
    fs::write(proj_root.join("a/file"), b"A-modified").unwrap();

    let changed = incremental_build(proj_root, &manifest, &store, &mut fps).unwrap();
    assert_eq!(changed, vec![0]);
}
