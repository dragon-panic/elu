use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn config_show_with_no_file_prints_empty_table() {
    let cfg = TempDir::new().unwrap();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .env("XDG_CONFIG_HOME", cfg.path())
        .args(["config", "show"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.is_empty() || s.trim().is_empty() || s.contains("# empty"));
}

#[test]
fn config_set_then_show_roundtrips() {
    let cfg = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .env("XDG_CONFIG_HOME", cfg.path())
        .args(["config", "set", "registry", "https://r.example.com"])
        .assert()
        .success();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .env("XDG_CONFIG_HOME", cfg.path())
        .args(["config", "show"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("registry"), "missing key in show: {s}");
    assert!(s.contains("https://r.example.com"), "missing value in show: {s}");
    assert!(fs::read_to_string(cfg.path().join("elu/config.toml")).unwrap().contains("https://r.example.com"));
}

#[test]
fn config_show_json_emits_object() {
    let cfg = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .env("XDG_CONFIG_HOME", cfg.path())
        .args(["config", "set", "store", "/tmp/x"])
        .assert()
        .success();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .env("XDG_CONFIG_HOME", cfg.path())
        .args(["--json", "config", "show"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["store"], "/tmp/x");
}
