//! Slice 3 of the registry round-trip arc (cx SnIt): the headline
//! integration test that motivated the whole arc.
//!
//! Publish a built fixture from a publisher store to an in-process axum
//! registry, then install it from a *fresh* subscriber store, and assert the
//! materialized output matches the publisher's source.

use std::fs;
use std::sync::Arc;

use assert_cmd::Command;
use elu_registry::blob_store::{BlobBackend, LocalBlobBackend};
use elu_registry::db::SqliteRegistryDb;
use elu_registry::server::{AppState, router};
use tempfile::TempDir;
use tokio::net::TcpListener;
use url::Url;

const MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "demo"
version     = "0.1.0"
kind        = "native"
description = "round-trip fixture"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

fn make_project(tmp: &TempDir) {
    fs::write(tmp.path().join("elu.toml"), MANIFEST).unwrap();
    fs::create_dir_all(tmp.path().join("layers/files")).unwrap();
    fs::write(tmp.path().join("layers/files/hello.txt"), "hi").unwrap();
}

/// Bind a free TCP port, build a real `LocalBlobBackend`, mount its router
/// (which serves both PUT and GET), and spawn. Subscriber-side install will
/// hit the same backend's GET handler to fetch bytes back.
async fn spawn_blob_backend() -> Arc<LocalBlobBackend> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
    let backend = Arc::new(LocalBlobBackend::new(base));
    let app = backend.clone().router();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    backend
}

async fn spawn_registry(state: Arc<AppState>) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap()
}

#[test]
fn publish_then_install_reproduces_original() {
    // 1. Publisher project: build the fixture.
    let pub_project = TempDir::new().unwrap();
    let pub_store = TempDir::new().unwrap();
    make_project(&pub_project);
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", pub_store.path().to_str().unwrap(), "build"])
        .current_dir(pub_project.path())
        .assert()
        .success();

    // 2. Spin up an in-process registry + a real LocalBlobBackend serving
    //    PUT and GET. The backend retains bytes so the subscriber can
    //    fetch them back.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let registry_url = rt.block_on(async {
        let backend = spawn_blob_backend().await;
        let state = Arc::new(AppState {
            db: SqliteRegistryDb::open_in_memory().unwrap(),
            blob_backend: backend as Arc<dyn BlobBackend>,
        });
        spawn_registry(state).await
    });

    // 3. Publish from the publisher's store.
    Command::cargo_bin("elu")
        .unwrap()
        .env_remove("ELU_PUBLISH_TOKEN")
        .args([
            "--store",
            pub_store.path().to_str().unwrap(),
            "--registry",
            registry_url.as_str(),
            "publish",
            "ns/demo@0.1.0",
            "--token",
            "alice",
        ])
        .current_dir(pub_project.path())
        .assert()
        .success();

    // 4. Subscriber: a *fresh* store and project dir. Install pulls
    //    everything back over HTTP.
    let sub_project = TempDir::new().unwrap();
    let sub_store = TempDir::new().unwrap();
    let out_path = sub_project.path().join("installed");
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            sub_store.path().to_str().unwrap(),
            "--registry",
            registry_url.as_str(),
            "install",
            "ns/demo@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .current_dir(sub_project.path())
        .assert()
        .success();

    // 5. The materialized output must contain the same file the publisher
    //    started from, byte-for-byte.
    let installed = fs::read_to_string(out_path.join("hello.txt"))
        .expect("installed hello.txt should exist");
    assert_eq!(installed, "hi", "round-trip must reproduce file contents");
}

#[test]
fn install_offline_errors_with_network_exit() {
    // `--offline` forbids registry contact; install must refuse rather than
    // silently fall back to store-only resolution.
    let store = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("installed");
    let assert = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "--offline",
            "install",
            "ns/demo@0.1.0",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .failure();
    assert_eq!(assert.get_output().status.code(), Some(4)); // CliError::Network
}
