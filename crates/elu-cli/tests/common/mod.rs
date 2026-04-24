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
        todo!("Env::new — create two TempDirs")
    }

    pub fn project_path(&self) -> &Path {
        self.project.path()
    }

    pub fn store_path(&self) -> &Path {
        self.store.path()
    }

    /// `elu --store <store> <args...>`. No cwd is set.
    pub fn elu(&self, _args: &[&str]) -> Command {
        todo!("Env::elu — build assert_cmd::Command with --store prepended")
    }

    /// Same as `elu`, with cwd = project path. For subcommands like `build`
    /// that read `./elu.toml`.
    pub fn elu_in_project(&self, _args: &[&str]) -> Command {
        todo!("Env::elu_in_project — like elu but current_dir=project")
    }

    /// Run `elu --store <store> --json <args...>` with cwd = project.
    /// Parse the LAST stdout line as JSON and return it (the `done` event).
    pub fn elu_json_done(&self, _args: &[&str]) -> serde_json::Value {
        todo!("Env::elu_json_done — run with --json, parse last stdout line")
    }
}

/// Write the canonical tiny fixture into `env.project_path()`:
///
/// - `elu.toml` for package `ns/demo@0.1.0`, kind `native`, one layer `files`.
/// - `layers/files/hello.txt` with contents `hi`.
///
/// Overwrites any existing files. Safe to call after `init` to replace the
/// scaffolded manifest.
pub fn tiny_fixture(_env: &Env) {
    todo!("tiny_fixture — write elu.toml + layers/files/hello.txt")
}
