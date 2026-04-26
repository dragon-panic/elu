//! `elu explain --diff <old> <new>` integration test (cx WKIW.3idQ.utwP).
//!
//! Builds two manifests for the same package — `0.1.0` (no deps) and
//! `0.2.0` (depends on `ns/demo@0.1.0`) — then asserts the diff form
//! highlights both the version change and the new dependency.

use std::fs;

use assert_cmd::Command;

mod common;
use common::Env;

const DEMO_V1: &str = r#"schema = 1
[package]
namespace   = "ns"
name        = "demo"
version     = "0.1.0"
kind        = "native"
description = "demo v1"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

const CONSUMER_V1: &str = r#"schema = 1
[package]
namespace   = "ns"
name        = "consumer"
version     = "0.1.0"
kind        = "native"
description = "consumer v1"
"#;

const CONSUMER_V2_WITH_DEP: &str = r#"schema = 1
[package]
namespace   = "ns"
name        = "consumer"
version     = "0.2.0"
kind        = "native"
description = "consumer v2"

[[dependency]]
ref     = "ns/demo"
version = "0.1.0"
"#;

fn build_manifest(env: &Env, manifest_toml: &str) -> String {
    fs::write(env.project_path().join("elu.toml"), manifest_toml).unwrap();
    let layer_dir = env.project_path().join("layers/files");
    fs::create_dir_all(&layer_dir).unwrap();
    fs::write(layer_dir.join("x"), "data").unwrap();
    let v = env.elu_json_done(&["build"]);
    v["manifest_hash"].as_str().unwrap().to_string()
}

fn build_two_manifests() -> (Env, String, String) {
    let env = Env::new();
    let _demo_hash = build_manifest(&env, DEMO_V1);
    let old_hash = build_manifest(&env, CONSUMER_V1);
    let new_hash = build_manifest(&env, CONSUMER_V2_WITH_DEP);
    (env, old_hash, new_hash)
}

#[test]
fn explain_diff_text_reports_version_change_and_added_dependency() {
    let (env, old_hash, new_hash) = build_two_manifests();
    let out = env
        .elu(&["explain", "--diff", &old_hash, &new_hash])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("0.1.0") && s.contains("0.2.0"), "missing version change: {s}");
    assert!(s.contains("ns/demo"), "missing added dep ns/demo: {s}");
}

#[test]
fn explain_diff_json_emits_explain_diff_struct() {
    let (env, old_hash, new_hash) = build_two_manifests();
    let mut cmd = Command::cargo_bin("elu").unwrap();
    cmd.arg("--store").arg(env.store_path()).arg("--json");
    cmd.args(["explain", "--diff", &old_hash, &new_hash]);
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["version_change"], "0.1.0 -> 0.2.0");
    let added: Vec<&str> = v["dependencies_added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert!(added.iter().any(|s| s.contains("ns/demo")), "added: {added:?}");
}

#[test]
fn explain_diff_unknown_ref_exits_three() {
    let (env, old_hash, _new_hash) = build_two_manifests();
    env.elu(&["explain", "--diff", &old_hash, "ns/missing@9.9.9"])
        .assert()
        .failure()
        .code(3);
}
