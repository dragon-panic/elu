use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

const MANIFEST: &str = r#"
schema = 1
[package]
namespace   = "ns"
name        = "pkg"
version     = "0.3.0"
kind        = "native"
description = "test pkg"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

fn build(store: &TempDir) {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("elu.toml"), MANIFEST).unwrap();
    fs::create_dir_all(project.path().join("layers/files")).unwrap();
    fs::write(project.path().join("layers/files/x"), "data").unwrap();
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn gc_succeeds_on_clean_store() {
    let store = TempDir::new().unwrap();
    build(&store);
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "gc"])
        .assert()
        .success();
}

#[test]
fn gc_json_emits_done_event_with_stats() {
    let store = TempDir::new().unwrap();
    build(&store);
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "--json", "gc"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let last = std::str::from_utf8(&out.stdout).unwrap().lines().last().unwrap();
    let v: serde_json::Value = serde_json::from_str(last).unwrap();
    assert_eq!(v["event"], "done");
    assert!(v.get("bytes_freed").is_some());
}

/// Sorted (relpath, bytes) snapshot of every file under the store root,
/// used to assert byte-identity across `gc --dry-run`.
fn store_snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn walk(p: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>, root: &std::path::Path) {
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(&path, out, root);
                } else if let Ok(bytes) = fs::read(&path) {
                    let rel = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                    out.push((rel, bytes));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out, root);
    out.sort();
    out
}

#[test]
fn gc_dry_run_reports_plan_without_mutating_store() {
    let store = TempDir::new().unwrap();
    build(&store);
    // Orphan the manifest by removing its ref. Now the manifest blob and its
    // layer become unreachable.
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "refs",
            "rm",
            "ns/pkg/0.3.0",
        ])
        .assert()
        .success();

    let before = store_snapshot(store.path());
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "--json", "gc", "--dry-run"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let last = std::str::from_utf8(&out.stdout).unwrap().lines().last().unwrap();
    let v: serde_json::Value = serde_json::from_str(last).unwrap();
    assert_eq!(v["event"], "done");
    assert_eq!(v["dry_run"], true);
    assert!(
        v["objects_to_remove"].as_u64().unwrap() >= 1,
        "expected >=1 object to remove: {v}",
    );
    assert!(
        v["bytes_to_free"].as_u64().unwrap() > 0,
        "expected bytes_to_free > 0: {v}",
    );

    let after = store_snapshot(store.path());
    assert_eq!(before, after, "gc --dry-run must not mutate the store");
}

#[test]
fn fsck_succeeds_on_clean_store() {
    let store = TempDir::new().unwrap();
    build(&store);
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "fsck"])
        .assert()
        .success();
}

#[test]
fn fsck_reports_corruption_with_exit_five() {
    let store = TempDir::new().unwrap();
    build(&store);
    // Corrupt one blob to trigger a hash mismatch.
    let objects_dir = store.path().join("objects/sha256");
    let mut victim: Option<std::path::PathBuf> = None;
    'outer: for entry in fs::read_dir(&objects_dir).unwrap() {
        let entry = entry.unwrap();
        if let Some(sub) = fs::read_dir(entry.path()).unwrap().next() {
            victim = Some(sub.unwrap().path());
            break 'outer;
        }
    }
    let victim = victim.expect("at least one stored object");
    fs::write(&victim, b"corrupt").unwrap();

    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "fsck"])
        .assert()
        .failure()
        .code(5);
}

#[test]
fn fsck_help_lists_repair_flag() {
    let assert = Command::cargo_bin("elu").unwrap().args(["fsck", "--help"]).assert().success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(s.contains("--repair"), "fsck --help missing --repair: {s}");
}
