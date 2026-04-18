use std::fs;

use camino::Utf8Path;
use elu_author::build::{build, BuildOpts};
use elu_author::report::ErrorCode;
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;

const WITH_SENSITIVE: &str = r#"
schema = 1

[package]
namespace   = "dragon"
name        = "leaky"
version     = "0.1.0"
kind        = "native"
description = "A package that ships secrets"

[[layer]]
name    = "config"
include = ["config/**"]
"#;

fn prepare() -> (tempfile::TempDir, camino::Utf8PathBuf) {
    let proj = tempfile::tempdir().unwrap();
    let proj_root = Utf8Path::from_path(proj.path()).unwrap().to_path_buf();
    fs::write(proj_root.join("elu.toml"), WITH_SENSITIVE).unwrap();
    fs::create_dir_all(proj_root.join("config")).unwrap();
    fs::write(proj_root.join("config/.env"), b"KEY=1").unwrap();
    fs::write(proj_root.join("config/app.toml"), b"x").unwrap();
    (proj, proj_root)
}

#[test]
fn non_strict_warns_but_succeeds() {
    let (_proj, proj_root) = prepare();
    let store_dir = tempfile::tempdir().unwrap();
    let store = FsStore::init_with_fsync(
        Utf8Path::from_path(store_dir.path()).unwrap(),
        FsyncMode::Never,
    )
    .unwrap();

    let (report, artifact) = build(&proj_root, &store, &BuildOpts::default()).unwrap();
    assert!(report.ok);
    assert!(artifact.is_some());
    assert!(
        report
            .warnings
            .iter()
            .any(|d| d.code == ErrorCode::SensitivePattern),
        "expected a sensitive-pattern warning, got {:?}",
        report.warnings
    );
}

#[test]
fn strict_promotes_sensitive_warning_to_error() {
    let (_proj, proj_root) = prepare();
    let store_dir = tempfile::tempdir().unwrap();
    let store = FsStore::init_with_fsync(
        Utf8Path::from_path(store_dir.path()).unwrap(),
        FsyncMode::Never,
    )
    .unwrap();

    let (report, artifact) = build(
        &proj_root,
        &store,
        &BuildOpts {
            check_only: false,
            strict: true,
        },
    )
    .unwrap();
    assert!(!report.ok);
    assert!(artifact.is_none());
    assert!(
        report
            .errors
            .iter()
            .any(|d| d.code == ErrorCode::SensitivePattern)
    );
}
