use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn init_writes_elu_toml_for_native_kind() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "init",
            "--path",
            tmp.path().to_str().unwrap(),
            "--kind",
            "native",
            "--name",
            "demo",
            "--namespace",
            "ns",
        ])
        .assert()
        .success();
    let body = fs::read_to_string(tmp.path().join("elu.toml")).unwrap();
    assert!(body.contains("\"demo\""), "missing name: {body}");
    assert!(body.contains("\"ns\""), "missing namespace: {body}");
    assert!(body.contains("kind        = \"native\""), "missing kind: {body}");
}

#[test]
fn init_kind_required_when_template_absent_exits_two() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args(["init", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn check_on_valid_manifest_succeeds() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "init",
            "--path",
            tmp.path().to_str().unwrap(),
            "--kind",
            "native",
            "--name",
            "good",
            "--namespace",
            "ns",
        ])
        .assert()
        .success();
    fs::create_dir_all(tmp.path().join("layers/files")).unwrap();
    fs::write(tmp.path().join("layers/files/hello.txt"), "hi").unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args(["check", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn check_missing_manifest_exits_two() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args(["check", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn check_json_emits_report_object() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("elu.toml"), "this is not toml = { broken").unwrap();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--json", "check", "--path", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(!v["errors"].as_array().unwrap().is_empty());
}

#[test]
fn init_help_lists_template_and_from_flags() {
    let assert = Command::cargo_bin("elu").unwrap().args(["init", "--help"]).assert().success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for f in ["--kind", "--name", "--namespace", "--from", "--template", "--path"] {
        assert!(s.contains(f), "init --help missing {f}: {s}");
    }
    let _ = predicate::str::contains("--kind").eval(&s);
}
