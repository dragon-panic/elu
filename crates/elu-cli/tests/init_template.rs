//! `elu init --template <ns/name@ver>` integration test (cx WKIW.3idQ.kbdL).
//!
//! Publish a built fixture to an in-process registry, then run `elu init
//! --template` and assert the registry-served template manifest is
//! materialized into the target directory along with the scaffold files
//! from its layer.

use std::fs;
use std::sync::Arc;

use assert_cmd::Command;
use elu_registry::blob_store::{BlobBackend, LocalBlobBackend};
use elu_registry::db::SqliteRegistryDb;
use elu_registry::server::{router, AppState};
use tempfile::TempDir;
use tokio::net::TcpListener;
use url::Url;

const TEMPLATE_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "tmpl"
version     = "0.1.0"
kind        = "native"
description = "init --template fixture"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

fn make_template_project(tmp: &TempDir) {
    fs::write(tmp.path().join("elu.toml"), TEMPLATE_MANIFEST).unwrap();
    fs::create_dir_all(tmp.path().join("layers/files")).unwrap();
    fs::write(tmp.path().join("layers/files/scaffold.txt"), "scaffolded").unwrap();
}

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
fn init_template_fetches_and_scaffolds_from_registry() {
    // 1. Publisher: build the template fixture.
    let pub_project = TempDir::new().unwrap();
    let pub_store = TempDir::new().unwrap();
    make_template_project(&pub_project);
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", pub_store.path().to_str().unwrap(), "build"])
        .current_dir(pub_project.path())
        .assert()
        .success();

    // 2. Stand up an in-process registry.
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

    // 3. Publish the template package.
    Command::cargo_bin("elu")
        .unwrap()
        .env_remove("ELU_PUBLISH_TOKEN")
        .args([
            "--store",
            pub_store.path().to_str().unwrap(),
            "--registry",
            registry_url.as_str(),
            "publish",
            "ns/tmpl@0.1.0",
            "--token",
            "alice",
        ])
        .current_dir(pub_project.path())
        .assert()
        .success();

    // 4. New target directory; init --template fetches from the registry.
    let target = TempDir::new().unwrap();
    Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--registry",
            registry_url.as_str(),
            "init",
            "--template",
            "ns/tmpl@0.1.0",
            "--path",
            target.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // 5. The scaffolded file from the template's layer must be present.
    let scaffolded = fs::read_to_string(target.path().join("scaffold.txt"))
        .expect("template scaffolded scaffold.txt should exist");
    assert_eq!(scaffolded, "scaffolded");
}
