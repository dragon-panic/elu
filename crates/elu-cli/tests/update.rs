//! `elu update` integration tests (cx WKIW.wX0h.jUbi).

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
description = "consumes upstream"

[[dependency]]
ref     = "ns/demo"
version = "*"
"#;

/// Build ns/demo into the env's shared store, then write a consumer
/// manifest that depends on ns/demo. No lockfile is written yet.
fn fixture_with_demo_dep(env: &Env) {
    tiny_fixture(env);
    env.elu_in_project(&["build"]).assert().success();
    fs::write(env.project_path().join("elu.toml"), CONSUMER_MANIFEST)
        .expect("rewrite consumer manifest");
}

#[test]
fn update_writes_lockfile_when_none_exists() {
    let env = Env::new();
    fixture_with_demo_dep(&env);
    assert!(
        !env.project_path().join("elu.lock").exists(),
        "fixture precondition: lockfile should not exist yet",
    );

    env.elu_in_project(&["update"]).assert().success();

    let lockfile = fs::read_to_string(env.project_path().join("elu.lock")).unwrap();
    assert!(
        lockfile.contains("name = \"demo\""),
        "lockfile missing ns/demo entry:\n{lockfile}",
    );
}

#[test]
fn update_named_target_in_manifest_succeeds() {
    let env = Env::new();
    fixture_with_demo_dep(&env);
    env.elu_in_project(&["update", "ns/demo"]).assert().success();

    let lockfile = fs::read_to_string(env.project_path().join("elu.lock")).unwrap();
    assert!(
        lockfile.contains("name = \"demo\""),
        "lockfile missing ns/demo after named update:\n{lockfile}",
    );
}

#[test]
fn update_unknown_target_errors_three() {
    let env = Env::new();
    fixture_with_demo_dep(&env);

    let result = env
        .elu_in_project(&["update", "ns/missing"])
        .assert()
        .failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(3),
        "unknown update target should exit 3 (resolution); got {code:?}",
    );
}

#[test]
fn update_with_no_manifest_exits_two() {
    let env = Env::new();
    let result = env.elu_in_project(&["update"]).assert().failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(2),
        "no manifest should exit 2 (usage); got {code:?}",
    );
}

#[test]
fn update_walks_up_from_subdirectory() {
    let env = Env::new();
    fixture_with_demo_dep(&env);
    let sub = env.project_path().join("nested/sub");
    fs::create_dir_all(&sub).unwrap();

    Command::cargo_bin("elu")
        .unwrap()
        .arg("--store")
        .arg(env.store_path())
        .arg("update")
        .current_dir(&sub)
        .assert()
        .success();

    assert!(
        env.project_path().join("elu.lock").exists(),
        "lockfile should be at project root after walk-up",
    );
    assert!(
        !sub.join("elu.lock").exists(),
        "lockfile should not be in subdirectory",
    );
}
