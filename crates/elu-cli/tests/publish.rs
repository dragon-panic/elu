//! Slice 2 of the registry round-trip arc (cx 7u2u): CLI publish dispatch.
//!
//! Drives the real `elu` binary against an in-process axum registry plus
//! `LocalBlobBackend`'s built-in PUT/GET router. Pre-builds a fixture via
//! `elu build`, then runs `elu publish`, then asserts the published
//! `PackageRecord` is visible in the registry's DB.

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
description = "publish-cli fixture"

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

async fn spawn_registry(state: Arc<AppState>) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap()
}

/// Spawn `LocalBlobBackend`'s router on its own listener; return the backend
/// (its `upload_url`/`download_url` point at this listener).
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

#[test]
fn publish_dispatch_pushes_built_package_to_registry() {
    // 1. Project + store dirs and a built fixture.
    let project = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    make_project(&project);
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(project.path())
        .assert()
        .success();

    // 2. Spawn the in-process registry + blob backend on a small tokio rt.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let (registry_url, state) = rt.block_on(async {
        let backend = spawn_blob_backend().await;
        let state = Arc::new(AppState {
            db: SqliteRegistryDb::open_in_memory().unwrap(),
            blob_backend: backend as Arc<dyn BlobBackend>,
        });
        let url = spawn_registry(state.clone()).await;
        (url, state)
    });

    // 3. Run `elu publish ns/demo@0.1.0 --token alice --registry <url>`.
    let assert = Command::cargo_bin("elu")
        .unwrap()
        .env_remove("ELU_PUBLISH_TOKEN")
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "--registry",
            registry_url.as_str(),
            "publish",
            "ns/demo@0.1.0",
            "--token",
            "alice",
        ])
        .current_dir(project.path())
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("ns/demo@0.1.0"),
        "publish stdout missing reference: {stdout}",
    );

    // 4. Assert the registry's DB now contains the version we just published.
    let record = state
        .db
        .get_version("ns", "demo", "0.1.0")
        .expect("version present");
    assert_eq!(record.namespace, "ns");
    assert_eq!(record.name, "demo");
    assert_eq!(record.version, "0.1.0");
    assert_eq!(record.publisher, "alice");
    assert_eq!(record.layers.len(), 1);
}

#[test]
fn publish_json_emits_published_event() {
    let project = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    make_project(&project);
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(project.path())
        .assert()
        .success();

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

    let assert = Command::cargo_bin("elu")
        .unwrap()
        .env_remove("ELU_PUBLISH_TOKEN")
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "--registry",
            registry_url.as_str(),
            "--json",
            "publish",
            "ns/demo@0.1.0",
            "--token",
            "alice",
        ])
        .current_dir(project.path())
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let last = stdout.lines().last().expect("at least one stdout line");
    let v: serde_json::Value = serde_json::from_str(last)
        .unwrap_or_else(|e| panic!("last stdout line not JSON: {last:?} ({e})"));
    assert_eq!(v["event"], "published");
    assert_eq!(v["namespace"], "ns");
    assert_eq!(v["name"], "demo");
    assert_eq!(v["version"], "0.1.0");
    assert!(v["manifest_blob_id"].as_str().is_some());
}
