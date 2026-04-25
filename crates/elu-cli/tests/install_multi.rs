//! WKIW.MqEx — multi-ref `install` + transitive registry resolution.
//!
//! Headline test: install a package whose dependency must be fetched
//! transitively from the registry. Pre-MqEx, install errors at
//! `install.rs:77-85` because the resolver's fetch_plan still has items
//! after the single-ref fetch. Post-MqEx, install drives the resolver
//! against a registry-backed source and walks the full closure into the
//! local store before stacking.

use std::fs;
use std::sync::Arc;

use assert_cmd::Command;
use elu_registry::blob_store::{BlobBackend, LocalBlobBackend};
use elu_registry::db::SqliteRegistryDb;
use elu_registry::server::{AppState, router};
use tempfile::TempDir;
use tokio::net::TcpListener;
use url::Url;

const DEP_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "dep"
version     = "0.1.0"
kind        = "native"
description = "transitive dep fixture"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

const APP_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "app"
version     = "0.1.0"
kind        = "native"
description = "fixture that depends on ns/dep"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"

[[dependency]]
ref     = "ns/dep"
version = "^0.1"
"#;

fn write_project(tmp: &TempDir, manifest: &str, marker_filename: &str, marker_contents: &str) {
    fs::write(tmp.path().join("elu.toml"), manifest).unwrap();
    fs::create_dir_all(tmp.path().join("layers/files")).unwrap();
    fs::write(tmp.path().join("layers/files").join(marker_filename), marker_contents).unwrap();
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

fn build(project: &TempDir, store: &TempDir) {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["--store", store.path().to_str().unwrap(), "build"])
        .current_dir(project.path())
        .assert()
        .success();
}

fn publish(project: &TempDir, store: &TempDir, registry: &Url, reference: &str) {
    Command::cargo_bin("elu")
        .unwrap()
        .env_remove("ELU_PUBLISH_TOKEN")
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "--registry",
            registry.as_str(),
            "publish",
            reference,
            "--token",
            "alice",
        ])
        .current_dir(project.path())
        .assert()
        .success();
}

#[test]
fn install_pulls_transitive_deps() {
    // Publisher: build + publish ns/dep@0.1.0, then ns/app@0.1.0 (depends on ns/dep).
    let pub_store = TempDir::new().unwrap();

    let dep_proj = TempDir::new().unwrap();
    write_project(&dep_proj, DEP_MANIFEST, "dep.txt", "from-dep");

    let app_proj = TempDir::new().unwrap();
    write_project(&app_proj, APP_MANIFEST, "app.txt", "from-app");

    build(&dep_proj, &pub_store);
    build(&app_proj, &pub_store);

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

    publish(&dep_proj, &pub_store, &registry_url, "ns/dep@0.1.0");
    publish(&app_proj, &pub_store, &registry_url, "ns/app@0.1.0");

    // Subscriber: fresh store. Install ns/app — its dep must come along.
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
            "ns/app@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .current_dir(sub_project.path())
        .assert()
        .success();

    // Both layers must materialize: app.txt from ns/app and dep.txt from ns/dep.
    let app_file = fs::read_to_string(out_path.join("app.txt"))
        .expect("app.txt should exist after install");
    assert_eq!(app_file, "from-app");
    let dep_file = fs::read_to_string(out_path.join("dep.txt"))
        .expect("dep.txt should exist after install (transitive dep was fetched)");
    assert_eq!(dep_file, "from-dep");
}

const A_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "a"
version     = "0.1.0"
kind        = "native"
description = "independent package A"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

const C_MANIFEST: &str = r#"schema = 1

[package]
namespace   = "ns"
name        = "c"
version     = "0.1.0"
kind        = "native"
description = "independent package C"

[[layer]]
name    = "files"
include = ["layers/files/**"]
strip   = "layers/files/"
"#;

#[test]
fn install_accepts_multiple_independent_refs() {
    let pub_store = TempDir::new().unwrap();

    let a_proj = TempDir::new().unwrap();
    write_project(&a_proj, A_MANIFEST, "a.txt", "from-a");
    let c_proj = TempDir::new().unwrap();
    write_project(&c_proj, C_MANIFEST, "c.txt", "from-c");

    build(&a_proj, &pub_store);
    build(&c_proj, &pub_store);

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

    publish(&a_proj, &pub_store, &registry_url, "ns/a@0.1.0");
    publish(&c_proj, &pub_store, &registry_url, "ns/c@0.1.0");

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
            "ns/a@0.1.0",
            "ns/c@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .current_dir(sub_project.path())
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(out_path.join("a.txt")).expect("a.txt"),
        "from-a"
    );
    assert_eq!(
        fs::read_to_string(out_path.join("c.txt")).expect("c.txt"),
        "from-c"
    );
}

#[test]
fn install_locked_rejects_when_resolution_changes_lockfile() {
    // Publish ns/a + ns/c. Write a stale lockfile that pins only ns/a.
    // `elu install ns/a ns/c --locked` must refuse: ns/c is a new pin the
    // lockfile doesn't cover. Exit code 7 = CliError::Lockfile.
    let pub_store = TempDir::new().unwrap();

    let a_proj = TempDir::new().unwrap();
    write_project(&a_proj, A_MANIFEST, "a.txt", "from-a");
    let c_proj = TempDir::new().unwrap();
    write_project(&c_proj, C_MANIFEST, "c.txt", "from-c");

    build(&a_proj, &pub_store);
    build(&c_proj, &pub_store);

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

    publish(&a_proj, &pub_store, &registry_url, "ns/a@0.1.0");
    publish(&c_proj, &pub_store, &registry_url, "ns/c@0.1.0");

    // Subscriber project with elu.toml (so lockfile discovery walks here)
    // and a stale elu.lock that names only ns/a.
    let sub_project = TempDir::new().unwrap();
    let sub_store = TempDir::new().unwrap();
    fs::write(
        sub_project.path().join("elu.toml"),
        r#"schema = 1

[package]
namespace   = "ns"
name        = "subscriber"
version     = "0.0.0"
kind        = "native"
description = "subscriber root"
"#,
    )
    .unwrap();
    // Lockfile pins a NEUTRAL package (ns/unrelated) the resolver won't
    // touch — install only walks ns/a + ns/c. The --locked check fires
    // post-resolution: ns/a and ns/c are both new pins, so it refuses.
    fs::write(
        sub_project.path().join("elu.lock"),
        r#"schema = 1

[[package]]
namespace = "ns"
name = "unrelated"
version = "0.1.0"
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
"#,
    )
    .unwrap();

    let out_path = sub_project.path().join("installed");
    let assert = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            sub_store.path().to_str().unwrap(),
            "--registry",
            registry_url.as_str(),
            "--locked",
            "install",
            "ns/a@0.1.0",
            "ns/c@0.1.0",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .current_dir(sub_project.path())
        .assert()
        .failure();
    assert_eq!(assert.get_output().status.code(), Some(7));
}
