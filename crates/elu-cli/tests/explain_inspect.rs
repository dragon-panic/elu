use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

const MANIFEST: &str = r#"
schema = 1
[package]
namespace   = "ns"
name        = "demo"
version     = "1.0.0"
kind        = "native"
description = "demo pkg for explain"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

fn build(store: &TempDir) -> String {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("elu.toml"), MANIFEST).unwrap();
    fs::create_dir_all(project.path().join("layers/files")).unwrap();
    fs::write(project.path().join("layers/files/x"), "data").unwrap();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "--json", "build"])
        .current_dir(project.path())
        .output()
        .unwrap();
    let last = std::str::from_utf8(&out.stdout).unwrap().lines().last().unwrap().to_string();
    let v: serde_json::Value = serde_json::from_str(&last).unwrap();
    v["manifest_hash"].as_str().unwrap().to_string()
}

#[test]
fn explain_by_namespace_name_version_prints_summary() {
    let store = TempDir::new().unwrap();
    build(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "explain",
            "ns/demo@1.0.0",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("ns/demo @ 1.0.0"), "missing header: {s}");
    assert!(s.contains("demo pkg for explain"), "missing description: {s}");
}

#[test]
fn explain_by_hash_prints_summary() {
    let store = TempDir::new().unwrap();
    let hash = build(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "explain", &hash])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("ns/demo @ 1.0.0"), "missing header: {s}");
}

#[test]
fn explain_unknown_ref_exits_three() {
    let store = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "explain",
            "ns/missing@9.9.9",
        ])
        .assert()
        .failure()
        .code(3);
}

#[test]
fn explain_range_ref_exits_three() {
    let store = TempDir::new().unwrap();
    build(&store);
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "explain",
            "ns/demo@^1",
        ])
        .assert()
        .failure()
        .code(3);
}

#[test]
fn inspect_json_returns_manifest_object() {
    let store = TempDir::new().unwrap();
    build(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "--json",
            "inspect",
            "ns/demo@1.0.0",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["package"]["name"], "demo");
    assert_eq!(v["package"]["namespace"], "ns");
}

#[test]
fn inspect_help_lists_arguments() {
    let assert = Command::cargo_bin("elu")
        .unwrap()
        .args(["inspect", "--help"])
        .assert()
        .success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(s.contains("REFERENCE"), "inspect --help missing REFERENCE: {s}");
}
