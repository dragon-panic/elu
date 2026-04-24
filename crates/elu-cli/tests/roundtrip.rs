//! Slice 3 of the registry round-trip arc (cx SnIt): the headline
//! integration test that motivated the whole arc.
//!
//! Publish a built fixture from a publisher store to an in-process axum
//! registry, then install it from a *fresh* subscriber store, and assert the
//! materialized output matches the publisher's source.

use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use assert_cmd::Command;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::put;
use elu_registry::blob_store::BlobBackend;
use elu_registry::db::SqliteRegistryDb;
use elu_registry::error::RegistryError;
use elu_registry::server::{AppState, router};
use elu_store::hash::BlobId;
use http_body_util::BodyExt;
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

/// A blob backend that stores bytes in memory, so the same HTTP server can
/// serve them back via GET. The publish-only `LocalBlobBackend` used by
/// `tests/publish.rs` only tracks "uploaded" status; for round-trip we
/// actually need the bytes.
#[derive(Clone)]
struct InMemoryBlobBackend {
    base_url: Url,
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemoryBlobBackend {
    fn new(base_url: Url) -> Self {
        Self {
            base_url,
            blobs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl BlobBackend for InMemoryBlobBackend {
    fn upload_url(&self, blob_id: &BlobId) -> Result<Url, RegistryError> {
        self.base_url
            .join(&format!("blobs/{}", blob_id))
            .map_err(|e| RegistryError::BlobBackend(e.to_string()))
    }

    fn download_url(&self, blob_id: &BlobId) -> Result<Url, RegistryError> {
        self.base_url
            .join(&format!("blobs/{}", blob_id))
            .map_err(|e| RegistryError::BlobBackend(e.to_string()))
    }

    fn has_blob(&self, blob_id: &BlobId) -> Result<bool, RegistryError> {
        Ok(self.blobs.lock().unwrap().contains_key(&blob_id.to_string()))
    }

    fn mark_uploaded(&self, _blob_id: &BlobId) -> Result<(), RegistryError> {
        // We mark on the PUT handler, where we actually have the bytes.
        Ok(())
    }
}

#[derive(Clone)]
struct BlobServerState {
    backend: InMemoryBlobBackend,
}

async fn put_blob_handler(
    State(state): State<BlobServerState>,
    Path(blob_id_str): Path<String>,
    body: Body,
) -> StatusCode {
    let blob_id: BlobId = match blob_id_str.parse() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    let bytes = match body.collect().await {
        Ok(c) => c.to_bytes(),
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    let mut hasher = elu_store::hasher::Hasher::new();
    hasher.update(&bytes);
    let actual = hasher.finalize();
    if BlobId(actual) != blob_id {
        return StatusCode::BAD_REQUEST;
    }
    state
        .backend
        .blobs
        .lock()
        .unwrap()
        .insert(blob_id.to_string(), bytes.to_vec());
    StatusCode::OK
}

async fn get_blob_handler(
    State(state): State<BlobServerState>,
    Path(blob_id_str): Path<String>,
) -> Response {
    match state.backend.blobs.lock().unwrap().get(&blob_id_str) {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(bytes.clone()))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    }
}

async fn spawn_blob_server() -> InMemoryBlobBackend {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
    let backend = InMemoryBlobBackend::new(base.clone());
    let app = Router::new()
        .route(
            "/blobs/{blob_id}",
            put(put_blob_handler).get(get_blob_handler),
        )
        .with_state(BlobServerState {
            backend: backend.clone(),
        });
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

    // 2. Spin up an in-process registry + GET/PUT blob server. The blob
    //    server retains bytes so the subscriber can fetch them back.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let registry_url = rt.block_on(async {
        let backend = spawn_blob_server().await;
        let state = Arc::new(AppState {
            db: SqliteRegistryDb::open_in_memory().unwrap(),
            blob_backend: Arc::new(backend) as Arc<dyn BlobBackend>,
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
