use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

const PROJECT_MANIFEST: &str = r#"
schema = 1
[package]
namespace   = "ns"
name        = "stackpkg"
version     = "0.1.0"
kind        = "native"
description = "test pkg"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

struct Project {
    _tmp: TempDir,
    store: TempDir,
}

fn build_project() -> Project {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("elu.toml"), PROJECT_MANIFEST).unwrap();
    fs::create_dir_all(tmp.path().join("layers/files")).unwrap();
    fs::write(tmp.path().join("layers/files/hello.txt"), "hi").unwrap();
    let store = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(tmp.path())
        .assert()
        .success();
    Project { _tmp: tmp, store }
}

#[test]
fn stack_infers_dir_format_from_plain_path() {
    let p = build_project();
    let out_tmp = TempDir::new().unwrap();
    let out_path = out_tmp.path().join("out");

    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            p.store.path().to_str().unwrap(),
            "stack",
            "ns/stackpkg@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(out_path.is_dir());
    assert_eq!(fs::read_to_string(out_path.join("hello.txt")).unwrap(), "hi");
}

#[test]
fn stack_infers_tar_format_from_extension() {
    let p = build_project();
    let out_tmp = TempDir::new().unwrap();
    let out_path = out_tmp.path().join("out.tar");

    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            p.store.path().to_str().unwrap(),
            "stack",
            "ns/stackpkg@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(out_path.is_file());
    // A couple of ustar bytes near the end indicate a valid tar.
    let bytes = fs::read(&out_path).unwrap();
    assert!(bytes.windows(5).any(|w| w == b"ustar"));
}

#[test]
fn stack_infers_tar_gz_format_and_compresses() {
    let p = build_project();
    let out_tmp = TempDir::new().unwrap();
    let out_path = out_tmp.path().join("out.tar.gz");

    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            p.store.path().to_str().unwrap(),
            "stack",
            "ns/stackpkg@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let bytes = fs::read(&out_path).unwrap();
    // gzip magic.
    assert_eq!(&bytes[..2], &[0x1f, 0x8b]);
}

#[test]
fn explicit_format_overrides_inference() {
    // Path ends in .tar but --format dir forces dir.
    let p = build_project();
    let out_tmp = TempDir::new().unwrap();
    let out_path = out_tmp.path().join("misleading.tar");

    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            p.store.path().to_str().unwrap(),
            "stack",
            "ns/stackpkg@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
            "--format",
            "dir",
        ])
        .assert()
        .success();
    assert!(out_path.is_dir(), "--format dir should produce a dir");
    assert_eq!(fs::read_to_string(out_path.join("hello.txt")).unwrap(), "hi");
}

#[test]
fn qcow2_requires_base() {
    let p = build_project();
    let out_tmp = TempDir::new().unwrap();
    let out_path = out_tmp.path().join("disk.qcow2");

    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            p.store.path().to_str().unwrap(),
            "stack",
            "ns/stackpkg@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .failure();
}
