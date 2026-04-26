//! `elu build --watch` integration test (cx WKIW.3idQ.jri6).
//!
//! Spawns the CLI in `--watch --json` mode, waits for the initial build's
//! `done` event, modifies an input file, and asserts a second `done` event
//! fires within a generous timeout.

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

const MANIFEST: &str = r#"schema = 1
[package]
namespace   = "ns"
name        = "watch-demo"
version     = "0.1.0"
kind        = "native"
description = "build --watch fixture"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

#[test]
fn build_watch_rebuilds_on_input_change() {
    let project = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    fs::write(project.path().join("elu.toml"), MANIFEST).unwrap();
    let layer_dir = project.path().join("layers/files");
    fs::create_dir_all(&layer_dir).unwrap();
    let watched_file = layer_dir.join("hello.txt");
    fs::write(&watched_file, "first").unwrap();

    let bin = assert_cmd::cargo::cargo_bin("elu");
    let mut child = Command::new(&bin)
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "--json",
            "build",
            "--watch",
        ])
        .current_dir(project.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn elu build --watch");

    let stdout = child.stdout.take().expect("child stdout");
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("\"event\":\"done\"") || line.contains("\"event\": \"done\"") {
                let _ = tx.send(line);
            }
        }
    });

    let first = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("initial build did not complete");
    assert!(first.contains("\"ok\":true"), "initial build not ok: {first}");

    // Trigger a rebuild by modifying an included file.
    thread::sleep(Duration::from_millis(200));
    fs::write(&watched_file, "second").unwrap();

    let second = match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(s) => s,
        Err(e) => {
            let _ = child.kill();
            let out = child.wait_with_output().ok();
            panic!(
                "rebuild did not fire ({e:?})\nchild stderr:\n{}",
                out.map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
                    .unwrap_or_default(),
            );
        }
    };
    assert!(second.contains("\"ok\":true"), "rebuild not ok: {second}");

    let _ = child.kill();
    let _ = child.wait();
}
