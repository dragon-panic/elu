//! Shared helpers for elu integration tests (fast tier: pure-Rust, tmpdir-only).
//!
//! The `elu` binary is the seam under test; these helpers just wire up fresh
//! tmpdirs and run the binary via `assert_cmd`.

#![allow(dead_code)]

use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

/// Paired fresh tmpdirs — one for the project, one for the content-addressed store.
pub struct Env {
    pub project: TempDir,
    pub store: TempDir,
}

impl Env {
    /// Fresh project and store tmpdirs. Each `Env` is fully isolated.
    pub fn new() -> Self {
        Self {
            project: TempDir::new().expect("create project tmpdir"),
            store: TempDir::new().expect("create store tmpdir"),
        }
    }

    pub fn project_path(&self) -> &Path {
        self.project.path()
    }

    pub fn store_path(&self) -> &Path {
        self.store.path()
    }

    /// `elu --store <store> <args...>`. No cwd is set.
    pub fn elu(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("elu").expect("cargo_bin elu");
        cmd.arg("--store").arg(self.store.path()).args(args);
        cmd
    }

    /// Same as `elu`, with cwd = project path. For subcommands like `build`
    /// that read `./elu.toml`.
    pub fn elu_in_project(&self, args: &[&str]) -> Command {
        let mut cmd = self.elu(args);
        cmd.current_dir(self.project.path());
        cmd
    }

    /// Run `elu --store <store> --json <args...>` with cwd = project.
    /// Parse the LAST stdout line as JSON and return it (the `done` event).
    pub fn elu_json_done(&self, args: &[&str]) -> serde_json::Value {
        let mut cmd = Command::cargo_bin("elu").expect("cargo_bin elu");
        cmd.arg("--store")
            .arg(self.store.path())
            .arg("--json")
            .args(args)
            .current_dir(self.project.path());
        let out = cmd.output().expect("run elu");
        assert!(
            out.status.success(),
            "elu failed: args={args:?}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        let stdout = std::str::from_utf8(&out.stdout).expect("elu stdout utf-8");
        let last = stdout
            .lines()
            .last()
            .unwrap_or_else(|| panic!("elu produced no stdout lines; args={args:?}"));
        serde_json::from_str(last)
            .unwrap_or_else(|e| panic!("last stdout line is not JSON: {last:?} ({e})"))
    }
}

/// Write the canonical tiny fixture into `env.project_path()`:
///
/// - `elu.toml` for package `ns/demo@0.1.0`, kind `native`, one layer `files`.
/// - `layers/files/hello.txt` with contents `hi`.
///
/// Overwrites any existing files. Safe to call after `init` to replace the
/// scaffolded manifest.
pub fn tiny_fixture(env: &Env) {
    const MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "demo"
version     = "0.1.0"
kind        = "native"
description = "happy-path fixture"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;
    std::fs::write(env.project_path().join("elu.toml"), MANIFEST).expect("write elu.toml");
    let layer_dir = env.project_path().join("layers/files");
    std::fs::create_dir_all(&layer_dir).expect("mkdir layers/files");
    std::fs::write(layer_dir.join("hello.txt"), "hi").expect("write hello.txt");
}
