//! Determinism tests: build and stack must produce the same bytes across
//! independent runs with identical inputs.
//!
//! These are a near-free oracle for a content-addressed builder — any stray
//! timestamp, permission bit, or nondeterministic iteration order will surface
//! as a hash or byte mismatch here. Unix-only for v1.

#![cfg(unix)]

mod common;

use common::{tiny_fixture, Env};

/// Run the canonical fixture through a fresh Env and return (env, manifest_hash).
fn build_fresh() -> (Env, String) {
    let env = Env::new();
    tiny_fixture(&env);
    let done = env.elu_json_done(&["build"]);
    let h = done["manifest_hash"]
        .as_str()
        .expect("manifest_hash in build done event")
        .to_string();
    (env, h)
}

#[test]
fn build_manifest_hash_is_stable() {
    let (_e1, h1) = build_fresh();
    let (_e2, h2) = build_fresh();
    assert_eq!(
        h1, h2,
        "manifest_hash must be stable across fresh stores for identical inputs"
    );
}

#[test]
fn tar_output_is_byte_identical() {
    let (e1, _) = build_fresh();
    let (e2, _) = build_fresh();

    let out1 = e1.project_path().join("out.tar");
    let out2 = e2.project_path().join("out.tar");

    e1.elu(&["stack", "ns/demo@0.1.0", "-o", out1.to_str().unwrap()])
        .assert()
        .success();
    e2.elu(&["stack", "ns/demo@0.1.0", "-o", out2.to_str().unwrap()])
        .assert()
        .success();

    let b1 = std::fs::read(&out1).expect("read out.tar #1");
    let b2 = std::fs::read(&out2).expect("read out.tar #2");
    assert_eq!(
        b1.len(),
        b2.len(),
        "tar sizes differ: {} vs {} bytes",
        b1.len(),
        b2.len()
    );
    assert_eq!(b1, b2, "tar bytes must be identical across independent builds");
}
