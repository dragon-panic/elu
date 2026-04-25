//! Slice 1 of the registry round-trip feature arc: the client publish library.
//!
//! Stand up the real axum registry router on a TCP port, stand up the real
//! `LocalBlobBackend` blob router on another port (verifies hash on PUT and
//! persists the bytes), build a fixture package via `elu build` so the store
//! holds a real canonical-JSON manifest plus its layer blob, drive
//! `publish_package`, and assert the returned `PackageRecord` matches the
//! server's DB view and that every blob landed in the backend.

use std::fs;
use std::sync::Arc;

use assert_cmd::Command;
use elu_registry::blob_store::{BlobBackend, LocalBlobBackend};
use elu_registry::client::fallback::RegistryClient;
use elu_registry::client::publish::publish_package;
use elu_registry::db::SqliteRegistryDb;
use elu_registry::server::{AppState, router};
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::hash::BlobId;
use elu_store::store::Store;
use tempfile::TempDir;
use tokio::net::TcpListener;
use url::Url;

/// Write a minimal `elu.toml` + a single layer file under `project_dir`. The
/// layer config matches the shape used by `crates/elu-cli/tests/publish.rs`.
fn make_project(project_dir: &std::path::Path, ns: &str, name: &str, version: &str) {
    let manifest = format!(
        r#"schema = 1

[package]
namespace   = "{ns}"
name        = "{name}"
version     = "{version}"
kind        = "native"
description = "Test package"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#
    );
    fs::write(project_dir.join("elu.toml"), manifest).unwrap();
    fs::create_dir_all(project_dir.join("layers/files")).unwrap();
    fs::write(
        project_dir.join("layers/files/hello.txt"),
        b"hello client publish",
    )
    .unwrap();
}

/// Build `ns/name@version` via the real `elu build` binary against `store`.
/// Returns the resulting layer blob id (looked up via the canonical-JSON
/// manifest the build wrote).
fn build_package(
    store: &FsStore,
    project_dir: &std::path::Path,
    ns: &str,
    name: &str,
    version: &str,
) -> BlobId {
    make_project(project_dir, ns, name, version);

    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.root().as_str(), "build"])
        .current_dir(project_dir)
        .assert()
        .success();

    // Read back the canonical-JSON manifest the build wrote and recover the
    // single layer's blob_id from its diff_id.
    let manifest_hash = store
        .get_ref(ns, name, version)
        .unwrap()
        .expect("ref present after build");
    let bytes = store
        .get_manifest(&manifest_hash)
        .unwrap()
        .expect("manifest present after build");
    let manifest: elu_manifest::Manifest =
        serde_json::from_slice(&bytes).expect("manifest is canonical JSON");
    assert_eq!(manifest.layers.len(), 1, "fixture has one layer");
    let diff_id = manifest.layers[0]
        .diff_id
        .as_ref()
        .expect("stored layer has diff_id");
    store
        .resolve_diff(diff_id)
        .unwrap()
        .expect("diff resolves to a stored blob")
}

/// Spawn the registry router on a free TCP port; return its base URL.
async fn spawn_registry(state: Arc<AppState>) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap()
}

/// Bind a free TCP port, build a real `LocalBlobBackend` whose `base_url`
/// resolves to that port, mount its router, and spawn. The backend handles
/// PUT (hash-verifying) and GET itself.
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

#[tokio::test]
async fn publish_package_end_to_end() {
    // ----- store + project: build a real fixture via `elu build` -----
    let store_dir = TempDir::new().unwrap();
    let store_root = camino::Utf8Path::from_path(store_dir.path()).unwrap();
    let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();
    let project_dir = TempDir::new().unwrap();

    let ns = "acme";
    let name = "widget";
    let version = "1.0.0";
    let blob_id = build_package(&store, project_dir.path(), ns, name, version);

    // ----- blob backend (real LocalBlobBackend, real PUT/GET) -----
    let backend = spawn_blob_backend().await;

    // ----- registry server -----
    let state = Arc::new(AppState {
        db: SqliteRegistryDb::open_in_memory().unwrap(),
        blob_backend: backend.clone() as Arc<dyn BlobBackend>,
    });
    let registry_url = spawn_registry(state.clone()).await;

    // ----- client -----
    let client = RegistryClient::new(vec![registry_url]);
    let record = publish_package(&client, &store, ns, name, version, "alice", None)
        .await
        .expect("publish succeeds");

    // ----- assertions -----
    assert_eq!(record.namespace, ns);
    assert_eq!(record.name, name);
    assert_eq!(record.version, version);
    assert_eq!(record.layers.len(), 1);
    assert_eq!(record.layers[0].blob_id, blob_id);
    assert_eq!(record.publisher, "alice");

    // Server DB sees the same record.
    let from_db = state.db.get_version(ns, name, version).unwrap();
    assert_eq!(from_db.namespace, ns);
    assert_eq!(from_db.name, name);
    assert_eq!(from_db.version, version);
    assert_eq!(from_db.layers.len(), 1);
    assert_eq!(from_db.layers[0].blob_id, blob_id);

    // Both blobs (layer + manifest) uploaded to the backend.
    assert!(
        backend.has_blob(&blob_id).unwrap(),
        "layer blob should be in backend",
    );
    let manifest_blob_id = BlobId(record.manifest_blob_id.0.clone());
    assert!(
        backend.has_blob(&manifest_blob_id).unwrap(),
        "manifest blob should be in backend",
    );
}
