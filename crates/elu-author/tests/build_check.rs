use std::fs;

use camino::Utf8Path;
use elu_author::build::{build, BuildOpts};
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::store::{RefFilter, Store};

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
"#;

#[test]
fn check_only_does_not_write_manifest_or_blob() {
    let proj = tempfile::tempdir().unwrap();
    let proj_root = Utf8Path::from_path(proj.path()).unwrap();
    fs::write(proj_root.join("elu.toml"), SIMPLE_TOML).unwrap();
    fs::create_dir_all(proj_root.join("target/release")).unwrap();
    fs::write(proj_root.join("target/release/hello"), b"X").unwrap();

    let store_dir = tempfile::tempdir().unwrap();
    let store_root = Utf8Path::from_path(store_dir.path()).unwrap();
    let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();

    let (report, artifact) = build(
        proj_root,
        &store,
        &BuildOpts {
            check_only: true,
            strict: false,
        },
    )
    .unwrap();

    assert!(report.ok);
    assert!(artifact.is_none(), "no artifact in check mode");
    assert!(
        store.list_refs(RefFilter::default()).unwrap().is_empty(),
        "no refs written"
    );
}

#[test]
fn check_only_still_surfaces_errors() {
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
include = ["target/release/does-not-exist"]
"#,
    )
    .unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store_root = Utf8Path::from_path(store_dir.path()).unwrap();
    let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();

    let (report, _artifact) = build(
        proj_root,
        &store,
        &BuildOpts {
            check_only: true,
            strict: false,
        },
    )
    .unwrap();
    assert!(!report.ok);
    assert!(
        report
            .errors
            .iter()
            .any(|d| d.code == elu_author::report::ErrorCode::LayerIncludeNoMatches)
    );
}
