//! `elu lock` integration tests (cx WKIW.wX0h.bP11).
//!
//! Slice 1 of the resolver-driven CLI surface arc. Covers:
//!   1. `elu lock` writes `elu.lock` next to the project's `elu.toml`.
//!   2. Walk-up: running `elu lock` from a subdirectory updates the
//!      project-root lockfile (cargo's rule, not literal CWD).
//!   3. No manifest under cwd or any ancestor → exit 2 (usage).
//!   4. `--locked` exits 7 when the would-be lockfile differs from disk.

use std::fs;

use assert_cmd::Command;

mod common;
use common::{Env, tiny_fixture};

#[test]
fn lock_writes_elu_lock_next_to_elu_toml() {
    let env = Env::new();
    tiny_fixture(&env);
    env.elu_in_project(&["lock"]).assert().success();
    let lockfile_path = env.project_path().join("elu.lock");
    assert!(
        lockfile_path.exists(),
        "elu.lock not created at project root: {lockfile_path:?}",
    );
    let body = fs::read_to_string(&lockfile_path).unwrap();
    assert!(
        body.contains("schema = 1"),
        "lockfile missing schema line: {body}",
    );
}

#[test]
fn lock_walks_up_from_subdirectory_to_find_manifest() {
    let env = Env::new();
    tiny_fixture(&env);
    let sub = env.project_path().join("nested/sub");
    fs::create_dir_all(&sub).unwrap();

    Command::cargo_bin("elu")
        .unwrap()
        .arg("--store")
        .arg(env.store_path())
        .arg("lock")
        .current_dir(&sub)
        .assert()
        .success();

    assert!(
        env.project_path().join("elu.lock").exists(),
        "lockfile should be at project root after walk-up",
    );
    assert!(
        !sub.join("elu.lock").exists(),
        "lockfile should not be in the subdirectory",
    );
}

#[test]
fn lock_with_no_manifest_exits_two() {
    let env = Env::new();
    // No elu.toml anywhere up the tree (Env's project tmpdir is fresh).
    let result = env.elu_in_project(&["lock"]).assert().failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(2),
        "no manifest should exit 2 (usage); got {code:?}",
    );
}

#[test]
fn lock_locked_errors_when_disk_lockfile_would_change() {
    let env = Env::new();
    tiny_fixture(&env);

    // Pre-seed a stale lockfile with a bogus entry. The fixture has no deps,
    // so the regenerated lockfile is empty — `--locked` must reject the diff.
    let stale = "schema = 1\n\
        \n\
        [[package]]\n\
        namespace = \"stale\"\n\
        name = \"ghost\"\n\
        version = \"0.0.0\"\n\
        hash = \"b3:0000000000000000000000000000000000000000000000000000000000000000\"\n";
    fs::write(env.project_path().join("elu.lock"), stale).unwrap();

    let result = env.elu_in_project(&["--locked", "lock"]).assert().failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(7),
        "--locked with diff should exit 7 (lockfile); got {code:?}",
    );
}
