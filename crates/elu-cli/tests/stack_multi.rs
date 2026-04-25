//! WKIW.ZGmX — `elu stack` accepts N refs.
//!
//! `stack` is offline (no registry fetch); both packages must already be
//! in the local store. The slice's job is just to drop the single-ref
//! guard at stack.rs:22-27 and pass all roots to the resolver.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

const A_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "a"
version     = "0.1.0"
kind        = "native"
description = "stack-multi A"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

const C_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "c"
version     = "0.1.0"
kind        = "native"
description = "stack-multi C"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

fn write_project(tmp: &TempDir, manifest: &str, marker: &str, contents: &str) {
    fs::write(tmp.path().join("elu.toml"), manifest).unwrap();
    fs::create_dir_all(tmp.path().join("layers/files")).unwrap();
    fs::write(tmp.path().join("layers/files").join(marker), contents).unwrap();
}

fn build(project: &TempDir, store: &TempDir) {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(project.path())
        .assert()
        .success();
}

fn shared_store_with_a_and_c() -> TempDir {
    let store = TempDir::new().unwrap();
    let a = TempDir::new().unwrap();
    let c = TempDir::new().unwrap();
    write_project(&a, A_MANIFEST, "a.txt", "from-a");
    write_project(&c, C_MANIFEST, "c.txt", "from-c");
    build(&a, &store);
    build(&c, &store);
    store
}

/// Walk `root` deterministically and return a sorted map of relative path →
/// file contents. Used for byte-identity assertions across runs.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let ft = entry.file_type().unwrap();
            if ft.is_dir() {
                stack.push((entry.path(), rel));
            } else if ft.is_file() {
                out.insert(rel, fs::read(entry.path()).unwrap());
            }
        }
    }
    out
}

#[test]
fn stack_accepts_multiple_independent_refs() {
    let store = shared_store_with_a_and_c();
    let out_tmp = TempDir::new().unwrap();
    let out = out_tmp.path().join("merged");

    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "stack",
            "ns/a@0.1.0",
            "ns/c@0.1.0",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(out.join("a.txt")).unwrap(), "from-a");
    assert_eq!(fs::read_to_string(out.join("c.txt")).unwrap(), "from-c");
}

#[test]
fn stack_multi_ref_is_deterministic() {
    let store = shared_store_with_a_and_c();
    let out1 = TempDir::new().unwrap();
    let out2 = TempDir::new().unwrap();
    let p1 = out1.path().join("merged");
    let p2 = out2.path().join("merged");

    for out in [&p1, &p2] {
        Command::cargo_bin("elu")
            .unwrap()
            .args([
                "--store",
                store.path().to_str().unwrap(),
                "stack",
                "ns/a@0.1.0",
                "ns/c@0.1.0",
                "-o",
                out.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    let s1 = snapshot(&p1);
    let s2 = snapshot(&p2);
    assert_eq!(s1, s2, "multi-ref stack must be byte-identical across runs");
}
