//! `elu add` integration tests (cx WKIW.wX0h.BfF6).
//!
//! Slice 2 of the resolver-driven CLI surface arc. Covers:
//!   1. `elu add ns/dep@*` appends a `[[dependency]]` entry to elu.toml
//!      and writes/refreshes elu.lock when the dep resolves.
//!   2. Idempotent: running the same `elu add` twice leaves the manifest
//!      with a single entry (and same content).
//!   3. Resolution failure (dep missing from the store) errors with exit 3
//!      and leaves the on-disk manifest unchanged — atomic semantics.
//!   4. No `elu.toml` under cwd or any ancestor → exit 2 (usage).
//!   5. Walk-up: running `elu add` from a subdirectory mutates the
//!      project-root manifest, not a stray subdir file.

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

/// Build the tiny ns/demo fixture into the env's shared store, then
/// rewrite elu.toml as a consumer project that may depend on ns/demo.
fn fixture_with_demo_in_store(env: &Env) {
    tiny_fixture(env);
    env.elu_in_project(&["build"]).assert().success();
    fs::write(env.project_path().join("elu.toml"), CONSUMER_MANIFEST)
        .expect("rewrite consumer manifest");
}

#[test]
fn add_appends_dependency_when_dep_is_in_store() {
    let env = Env::new();
    fixture_with_demo_in_store(&env);

    env.elu_in_project(&["add", "ns/demo@*"]).assert().success();

    let manifest = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();
    assert!(
        manifest.contains("ref = \"ns/demo\""),
        "manifest missing dep entry:\n{manifest}",
    );
    let lockfile = fs::read_to_string(env.project_path().join("elu.lock")).unwrap();
    assert!(
        lockfile.contains("ns/demo") || lockfile.contains("\"demo\""),
        "lockfile missing ns/demo:\n{lockfile}",
    );
}

#[test]
fn add_is_idempotent() {
    let env = Env::new();
    fixture_with_demo_in_store(&env);

    env.elu_in_project(&["add", "ns/demo@*"]).assert().success();
    let after_first = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();
    env.elu_in_project(&["add", "ns/demo@*"]).assert().success();
    let after_second = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();

    assert_eq!(
        after_first, after_second,
        "manifest changed on second `add` of same ref",
    );
    let dep_count = after_second.matches("ref = \"ns/demo\"").count();
    assert_eq!(dep_count, 1, "duplicate dep entries:\n{after_second}");
}

#[test]
fn add_unresolvable_dep_errors_three_and_does_not_mutate_manifest() {
    let env = Env::new();
    tiny_fixture(&env);
    let before = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();

    let result = env
        .elu_in_project(&["add", "missing/pkg@^1.0"])
        .assert()
        .failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(3),
        "unresolvable dep should exit 3 (resolution); got {code:?}",
    );

    let after = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();
    assert_eq!(before, after, "manifest must not mutate when resolve fails");
    assert!(
        !env.project_path().join("elu.lock").exists(),
        "lockfile must not be written when add fails",
    );
}

#[test]
fn add_with_no_manifest_exits_two() {
    let env = Env::new();
    let result = env
        .elu_in_project(&["add", "ns/demo@*"])
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
fn add_walks_up_from_subdirectory() {
    let env = Env::new();
    fixture_with_demo_in_store(&env);
    let sub = env.project_path().join("nested/sub");
    fs::create_dir_all(&sub).unwrap();

    Command::cargo_bin("elu")
        .unwrap()
        .arg("--store")
        .arg(env.store_path())
        .args(["add", "ns/demo@*"])
        .current_dir(&sub)
        .assert()
        .success();

    let manifest = fs::read_to_string(env.project_path().join("elu.toml")).unwrap();
    assert!(
        manifest.contains("ref = \"ns/demo\""),
        "project-root manifest not mutated:\n{manifest}",
    );
    assert!(
        !sub.join("elu.toml").exists(),
        "manifest should not be in subdirectory",
    );
    assert!(
        env.project_path().join("elu.lock").exists(),
        "lockfile should be at project root",
    );
}
