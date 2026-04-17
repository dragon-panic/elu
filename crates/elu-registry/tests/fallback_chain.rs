use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use elu_registry::client::fallback::RegistryClient;
use elu_registry::types::*;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use tokio::net::TcpListener;
use url::Url;

fn test_hash(b: u8) -> Hash {
    Hash::new(HashAlgo::Sha256, [b; 32])
}

fn sample_record() -> PackageRecord {
    PackageRecord {
        namespace: "acme".into(),
        name: "widget".into(),
        version: "1.0.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0xaa)),
        manifest_url: Url::parse("https://blobs.example/m").unwrap(),
        kind: Some("native".into()),
        description: Some("Widget".into()),
        tags: vec![],
        layers: vec![LayerRecord {
            diff_id: DiffId(test_hash(0xbb)),
            blob_id: BlobId(test_hash(0xcc)),
            url: Url::parse("https://blobs.example/b").unwrap(),
            size_compressed: 100,
            size_uncompressed: 200,
        }],
        publisher: "alice".into(),
        published_at: "2026-01-01T00:00:00Z".into(),
        signature: None,
        visibility: Visibility::Public,
    }
}

/// Start a test server that returns 404 for all package requests.
async fn start_empty_registry() -> Url {
    let app = Router::new().route(
        "/api/v1/packages/{ns}/{name}/{version}",
        get(|| async { StatusCode::NOT_FOUND }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap()
}

/// Start a test server that returns a known package record.
async fn start_registry_with_package() -> Url {
    let record = sample_record();
    let record = Arc::new(record);

    let app = Router::new().route(
        "/api/v1/packages/{ns}/{name}/{version}",
        get(move || {
            let r = record.clone();
            async move { axum::Json((*r).clone()) }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap()
}

#[tokio::test]
async fn fallback_chain_tries_second_registry_on_404() {
    let empty_url = start_empty_registry().await;
    let has_pkg_url = start_registry_with_package().await;

    let client = RegistryClient::new(vec![empty_url, has_pkg_url]);
    let result = client.fetch_package("acme", "widget", "1.0.0").await;
    assert!(result.is_ok());

    let record = result.unwrap();
    assert_eq!(record.namespace, "acme");
    assert_eq!(record.name, "widget");
}

#[tokio::test]
async fn fallback_chain_returns_error_if_all_registries_fail() {
    let empty1 = start_empty_registry().await;
    let empty2 = start_empty_registry().await;

    let client = RegistryClient::new(vec![empty1, empty2]);
    let result = client.fetch_package("acme", "nonexistent", "1.0.0").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn single_registry_returns_package() {
    let url = start_registry_with_package().await;
    let client = RegistryClient::new(vec![url]);
    let result = client.fetch_package("acme", "widget", "1.0.0").await;
    assert!(result.is_ok());
}
