use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

const VALID_MANIFEST: &str = r#"
schema = 1
[package]
namespace   = "ns"
name        = "pkg"
version     = "0.2.0"
kind        = "native"
description = "test pkg"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

fn build_into_store(store: &TempDir) -> String {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("elu.toml"), VALID_MANIFEST).unwrap();
    fs::create_dir_all(project.path().join("layers/files")).unwrap();
    fs::write(project.path().join("layers/files/x"), "hi").unwrap();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "--json", "build"])
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let last = std::str::from_utf8(&out.stdout).unwrap().lines().last().unwrap().to_string();
    let v: serde_json::Value = serde_json::from_str(&last).unwrap();
    v["manifest_hash"].as_str().unwrap().to_string()
}

#[test]
fn ls_lists_built_packages_human() {
    let store = TempDir::new().unwrap();
    build_into_store(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "ls"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("ns/pkg"), "expected ns/pkg in {s}");
    assert!(s.contains("0.2.0"), "expected version in {s}");
}

#[test]
fn ls_namespace_filter_excludes_others() {
    let store = TempDir::new().unwrap();
    build_into_store(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "ls", "other-ns"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(!s.contains("ns/pkg"), "ns/pkg should be filtered out: {s}");
}

#[test]
fn ls_json_emits_array() {
    let store = TempDir::new().unwrap();
    build_into_store(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "--json", "ls"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.is_array());
    assert_eq!(v.as_array().unwrap()[0]["namespace"], "ns");
}

#[test]
fn refs_ls_lists_refs() {
    let store = TempDir::new().unwrap();
    build_into_store(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "refs", "ls"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("ns/pkg/0.2.0"), "expected ref path: {s}");
}

#[test]
fn refs_set_then_get_via_ls() {
    let store = TempDir::new().unwrap();
    let hash = build_into_store(&store);
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "refs",
            "set",
            "ns/pkg/9.9.9",
            &hash,
        ])
        .assert()
        .success();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "refs", "ls"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("ns/pkg/9.9.9"), "missing new ref: {s}");
}

#[test]
fn refs_set_invalid_spec_exits_two() {
    let store = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "refs",
            "set",
            "bogus",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        ])
        .assert()
        .failure()
        .code(2);
}
