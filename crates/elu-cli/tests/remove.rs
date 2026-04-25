//! `elu remove` integration tests (cx WKIW.wX0h.cXJm).
//!
//! Slice 3 of the resolver-driven CLI surface arc. Inverse of `add`.

use std::fs;

use assert_cmd::Command;

mod common;
use common::{Env, tiny_fixture};

const CONSUMER_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "consumer"
version     = "0.1.0"
kind        = "native"
description = "consumes ns/demo"
"#;

/// Build ns/demo into the env's shared store, rewrite manifest to
/// ns/consumer, then `elu add ns/demo@*` so we have something to remove.
fn fixture_with_demo_added(env: &Env) {
    tiny_fixture(env);
    env.elu_in_project(&["build"]).assert().success();
    fs::write(env.project_path().join("elu.toml"), CONSUMER_MANIFEST)
        .expect("rewrite consumer manifest");
    env.elu_in_project(&["add", "ns/demo@*"]).assert().success();
}

#[test]
fn remove_strips_dependency_from_manifest() {
    let env = Env::new();
    fixture_with_demo_added(&env);
    assert!(
        fs::read_to_string(env.project_path().join("elu.toml"))
            .unwrap()
            .contains("ref = \"ns/demo\""),
        "fixture precondition: ns/demo dep entry should be present",
    );

    env.elu_in_project(&["remove", "ns/demo"]).assert().success();

    let manifest = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();
    assert!(
        !manifest.contains("ref = \"ns/demo\""),
        "manifest still has ns/demo dep entry after remove:\n{manifest}",
    );
}

#[test]
fn remove_unknown_package_errors_two() {
    let env = Env::new();
    tiny_fixture(&env);
    let before = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();

    let result = env
        .elu_in_project(&["remove", "ns/missing"])
        .assert()
        .failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(2),
        "removing absent dep should exit 2 (usage); got {code:?}",
    );

    let after = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();
    assert_eq!(before, after, "manifest must not mutate on error");
}

#[test]
fn remove_with_no_manifest_exits_two() {
    let env = Env::new();
    let result = env
        .elu_in_project(&["remove", "ns/foo"])
        .assert()
        .failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(2),
        "no manifest should exit 2 (usage); got {code:?}",
    );
}

#[test]
fn remove_walks_up_from_subdirectory() {
    let env = Env::new();
    fixture_with_demo_added(&env);
    let sub = env.project_path().join("nested/sub");
    fs::create_dir_all(&sub).unwrap();

    Command::cargo_bin("elu")
        .unwrap()
        .arg("--store")
        .arg(env.store_path())
        .args(["remove", "ns/demo"])
        .current_dir(&sub)
        .assert()
        .success();

    let manifest = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();
    assert!(
        !manifest.contains("ref = \"ns/demo\""),
        "project-root manifest still has ns/demo dep entry:\n{manifest}",
    );
}

#[test]
fn remove_locked_errors_when_manifest_would_change() {
    let env = Env::new();
    fixture_with_demo_added(&env);

    let result = env
        .elu_in_project(&["--locked", "remove", "ns/demo"])
        .assert()
        .failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(7),
        "--locked with diff should exit 7 (lockfile); got {code:?}",
    );
}
