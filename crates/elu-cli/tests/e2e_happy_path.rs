//! End-to-end happy path: init → build → inspect → stack (dir).
//!
//! Exercises every major crate seam in one shot. Fast tier: pure Rust, tmpdirs
//! only, no external binaries, no network. Unix-only for v1.

#![cfg(unix)]

mod common;

use common::{tiny_fixture, Env};

#[test]
fn init_build_inspect_stack() {
    let env = Env::new();

    // 1. `init` scaffolds a project directory with an elu.toml.
    env.elu(&[
        "init",
        "--path",
        env.project_path().to_str().unwrap(),
        "--kind",
        "native",
        "--name",
        "demo",
        "--namespace",
        "ns",
    ])
    .assert()
    .success();
    assert!(
        env.project_path().join("elu.toml").exists(),
        "init should have written elu.toml"
    );

    // 2. Overlay the canonical fixture so the manifest matches real layer content.
    tiny_fixture(&env);

    // 3. `build --json` emits a `done` event with a non-empty manifest_hash.
    let done = env.elu_json_done(&["build"]);
    let manifest_hash = done["manifest_hash"]
        .as_str()
        .expect("build --json done event should carry a manifest_hash");
    assert!(!manifest_hash.is_empty(), "manifest_hash must be non-empty");

    // 4. `inspect --json` returns the manifest; package identity round-trips.
    let out = env
        .elu(&["--json", "inspect", "ns/demo@0.1.0"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "inspect failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let manifest: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(manifest["package"]["name"], "demo");
    assert_eq!(manifest["package"]["namespace"], "ns");

    // 5. `stack -o <dir>` materializes the one layer file at the expected path.
    let stack_out = env.project_path().join("stack-out");
    env.elu(&[
        "stack",
        "ns/demo@0.1.0",
        "-o",
        stack_out.to_str().unwrap(),
    ])
    .assert()
    .success();
    let hello = std::fs::read_to_string(stack_out.join("hello.txt"))
        .expect("stack should materialize hello.txt");
    assert_eq!(hello, "hi");
}
