use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

const VALID_MANIFEST: &str = r#"
schema = 1
[package]
namespace   = "ns"
name        = "pkg"
version     = "0.1.0"
kind        = "native"
description = "test pkg"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

fn make_project(tmp: &TempDir) {
    fs::write(tmp.path().join("elu.toml"), VALID_MANIFEST).unwrap();
    fs::create_dir_all(tmp.path().join("layers/files")).unwrap();
    fs::write(tmp.path().join("layers/files/hello.txt"), "hi").unwrap();
}

#[test]
fn build_writes_manifest_to_store_in_current_dir() {
    let tmp = TempDir::new().unwrap();
    make_project(&tmp);
    let store = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(tmp.path())
        .assert()
        .success();
    // After build the store should contain a ref under ns/pkg/0.1.0.
    let ref_path = store.path().join("refs/ns/pkg/0.1.0");
    assert!(ref_path.exists(), "expected ref at {:?}", ref_path);
}

#[test]
fn build_check_only_does_not_write_ref() {
    let tmp = TempDir::new().unwrap();
    make_project(&tmp);
    let store = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build", "--check"])
        .current_dir(tmp.path())
        .assert()
        .success();
    let ref_path = store.path().join("refs/ns/pkg/0.1.0");
    assert!(!ref_path.exists(), "ref should not exist with --check");
}

#[test]
fn build_json_emits_artifact_event_with_manifest_hash() {
    let tmp = TempDir::new().unwrap();
    make_project(&tmp);
    let store = TempDir::new().unwrap();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "--json",
            "build",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // Last line should be a `done` event with a manifest_hash.
    let lines: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap()
        .lines()
        .collect();
    assert!(!lines.is_empty(), "expected json output");
    let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
    assert_eq!(last["event"], "done");
    assert_eq!(last["ok"], true);
    assert!(last["manifest_hash"].as_str().unwrap().starts_with("sha256:"));
}

#[test]
fn build_invalid_manifest_exits_two() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("elu.toml"), "broken = {").unwrap();
    let store = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .code(2);
}
