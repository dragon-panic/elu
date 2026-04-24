//! Slice 1 of the registry round-trip feature arc: the client publish library.
//!
//! Stand up the real axum registry router on a TCP port, stand up a tiny
//! blob-storage HTTP endpoint on another port (accepts PUT, marks the blob
//! uploaded on the shared `LocalBlobBackend`), seed an `FsStore` with a
//! manifest + one layer blob, drive `publish_package`, and assert the
//! returned `PackageRecord` matches the server's DB view and that every
//! blob landed in the backend.

use std::io::Cursor;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::put;
use elu_registry::blob_store::{BlobBackend, LocalBlobBackend};
use elu_registry::client::fallback::RegistryClient;
use elu_registry::client::publish::publish_package;
use elu_registry::db::SqliteRegistryDb;
use elu_registry::server::{AppState, router};
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::hash::BlobId;
use elu_store::store::Store;
use http_body_util::BodyExt;
use tokio::net::TcpListener;
use url::Url;

/// Build a minimal valid stored-form manifest TOML for `ns/name@version`
/// with a single layer of `tar_size` uncompressed bytes at `diff_id`.
fn make_manifest_toml(
    ns: &str,
    name: &str,
    version: &str,
    diff_id: &elu_store::hash::DiffId,
    tar_size: u64,
) -> String {
    format!(
        r#"schema = 1

[package]
namespace = "{ns}"
name = "{name}"
version = "{version}"
kind = "native"
description = "Test package"

[[layer]]
diff_id = "{diff_id}"
size = {tar_size}
"#
    )
}

/// Build a valid tar archive containing a single file. Returns raw tar bytes
/// (uncompressed — so blob_id == diff_id when stored).
fn make_tar_bytes(filename: &str, content: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_path(filename).unwrap();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, content).unwrap();
    builder.into_inner().unwrap()
}

/// Seed `store` with a manifest + one plain-tar layer for `ns/name@version`.
/// Returns the stored `blob_id` of the layer.
fn seed_package(store: &FsStore, ns: &str, name: &str, version: &str) -> BlobId {
    // 1. Put the layer blob (plain tar) first so we know diff_id.
    let tar_bytes = make_tar_bytes("hello.txt", b"hello client publish");
    let tar_size = tar_bytes.len() as u64;
    let put = store.put_blob(&mut Cursor::new(tar_bytes)).unwrap();

    // 2. Build + store the manifest referencing that diff_id.
    let manifest_toml = make_manifest_toml(ns, name, version, &put.diff_id, tar_size);
    let manifest_hash = store.put_manifest(manifest_toml.as_bytes()).unwrap();
    store.put_ref(ns, name, version, &manifest_hash).unwrap();

    put.blob_id
}

/// State shared with the blob-upload HTTP server.
#[derive(Clone)]
struct BlobUploadState {
    backend: Arc<LocalBlobBackend>,
}

/// Handler for `PUT /blobs/:blob_id` — parse the blob id, verify the uploaded
/// bytes hash to it, then mark it uploaded on the shared backend.
async fn put_blob_handler(
    State(state): State<BlobUploadState>,
    Path(blob_id_str): Path<String>,
    body: Body,
) -> StatusCode {
    let blob_id: BlobId = match blob_id_str.parse() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    // Verify content hash matches the blob_id — catches silent corruption and
    // proves the client uploaded the real bytes.
    let mut hasher = elu_store::hasher::Hasher::new();
    hasher.update(&bytes);
    let actual = hasher.finalize();
    if BlobId(actual) != blob_id {
        return StatusCode::BAD_REQUEST;
    }

    state.backend.mark_uploaded(&blob_id).unwrap();
    StatusCode::OK
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

/// Bind a TCP listener on a free port, build an upload server against it,
/// spawn, and return (base_url, backend). The backend's `upload_url` output
/// will point at this server so client PUTs land at `put_blob_handler`.
async fn spawn_blob_backend() -> (Url, Arc<LocalBlobBackend>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
    let backend = Arc::new(LocalBlobBackend::new(base.clone()));

    let app = Router::new()
        .route("/blobs/{blob_id}", put(put_blob_handler))
        .with_state(BlobUploadState {
            backend: backend.clone(),
        });
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (base, backend)
}

#[tokio::test]
async fn publish_package_end_to_end() {
    // ----- store: seed a manifest + one layer -----
    let store_dir = tempfile::TempDir::new().unwrap();
    let store_root = camino::Utf8Path::from_path(store_dir.path()).unwrap();
    let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();

    let ns = "acme";
    let name = "widget";
    let version = "1.0.0";
    let blob_id = seed_package(&store, ns, name, version);

    // ----- blob backend (and its HTTP upload receiver) -----
    let (_blob_base, backend) = spawn_blob_backend().await;

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
