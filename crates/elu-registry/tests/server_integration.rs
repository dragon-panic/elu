use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use elu_registry::blob_store::LocalBlobBackend;
use elu_registry::db::SqliteRegistryDb;
use elu_registry::server::{AppState, router};
use elu_registry::types::*;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use http_body_util::BodyExt;
use tower::util::ServiceExt;
use url::Url;

fn test_hash(b: u8) -> Hash {
    Hash::new(HashAlgo::Sha256, [b; 32])
}

fn test_app() -> Arc<AppState> {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    let blob_backend = Arc::new(LocalBlobBackend::new(
        Url::parse("http://localhost:9090/").unwrap(),
    ));
    Arc::new(AppState { db, blob_backend })
}

/// Build a minimal valid manifest TOML string for testing.
fn test_manifest_toml(ns: &str, name: &str, version: &str, diff_id: &DiffId) -> String {
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
size = 200
"#
    )
}

// ---- Slice 9: POST begin publish ----

#[tokio::test]
async fn begin_publish_returns_session_and_upload_urls() {
    let state = test_app();
    let app = router(state.clone());

    let diff_id = DiffId(test_hash(0xbb));
    let blob_id = BlobId(test_hash(0xcc));
    let manifest_blob_id = ManifestHash(test_hash(0xaa));
    let manifest_toml = test_manifest_toml("acme", "widget", "1.0.0", &diff_id);
    let manifest_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        manifest_toml.as_bytes(),
    );

    let req_body = serde_json::json!({
        "manifest_blob_id": manifest_blob_id.to_string(),
        "manifest": manifest_b64,
        "layers": [{
            "diff_id": diff_id.to_string(),
            "blob_id": blob_id.to_string(),
            "size_compressed": 100,
            "size_uncompressed": 200
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/acme/widget/1.0.0")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer alice")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: PublishResponse = serde_json::from_slice(&body).unwrap();
    assert!(!resp.session_id.is_empty());
    // Should have upload URLs for the blob and manifest
    assert!(!resp.upload_urls.is_empty());
}

#[tokio::test]
async fn begin_publish_rejects_reserved_namespace() {
    let state = test_app();
    let app = router(state.clone());

    let diff_id = DiffId(test_hash(0xbb));
    let manifest_toml = test_manifest_toml("debian", "pkg", "1.0.0", &diff_id);
    let manifest_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        manifest_toml.as_bytes(),
    );

    let req_body = serde_json::json!({
        "manifest_blob_id": ManifestHash(test_hash(0xaa)).to_string(),
        "manifest": manifest_b64,
        "layers": [{
            "diff_id": diff_id.to_string(),
            "blob_id": BlobId(test_hash(0xcc)).to_string(),
            "size_compressed": 100,
            "size_uncompressed": 200
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/debian/pkg/1.0.0")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer alice")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn begin_publish_requires_auth() {
    let state = test_app();
    let app = router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/acme/widget/1.0.0")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---- Slice 10: POST commit ----

#[tokio::test]
async fn commit_makes_package_visible() {
    let state = test_app();

    let diff_id = DiffId(test_hash(0xbb));
    let blob_id = BlobId(test_hash(0xcc));
    let manifest_blob_id = ManifestHash(test_hash(0xaa));
    let manifest_toml = test_manifest_toml("acme", "widget", "1.0.0", &diff_id);
    let manifest_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        manifest_toml.as_bytes(),
    );

    let req_body = serde_json::json!({
        "manifest_blob_id": manifest_blob_id.to_string(),
        "manifest": manifest_b64,
        "layers": [{
            "diff_id": diff_id.to_string(),
            "blob_id": blob_id.to_string(),
            "size_compressed": 100,
            "size_uncompressed": 200
        }]
    });

    // Begin publish
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/acme/widget/1.0.0")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer alice")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Simulate blob upload by marking as uploaded
    state.blob_backend.mark_uploaded(&blob_id).unwrap();
    state
        .blob_backend
        .mark_uploaded(&BlobId(manifest_blob_id.0.clone()))
        .unwrap();

    // Commit
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/acme/widget/1.0.0/commit")
                .header("Authorization", "Bearer alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let record: PackageRecord = serde_json::from_slice(&body).unwrap();
    assert_eq!(record.namespace, "acme");
    assert_eq!(record.name, "widget");
    assert_eq!(record.version, "1.0.0");

    // Now GET should work
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/packages/acme/widget/1.0.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn commit_fails_if_blobs_missing() {
    let state = test_app();

    let diff_id = DiffId(test_hash(0xbb));
    let blob_id = BlobId(test_hash(0xcc));
    let manifest_blob_id = ManifestHash(test_hash(0xaa));
    let manifest_toml = test_manifest_toml("acme", "widget", "1.0.0", &diff_id);
    let manifest_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        manifest_toml.as_bytes(),
    );

    let req_body = serde_json::json!({
        "manifest_blob_id": manifest_blob_id.to_string(),
        "manifest": manifest_b64,
        "layers": [{
            "diff_id": diff_id.to_string(),
            "blob_id": blob_id.to_string(),
            "size_compressed": 100,
            "size_uncompressed": 200
        }]
    });

    // Begin
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/acme/widget/1.0.0")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer alice")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Don't upload blobs, try to commit
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/acme/widget/1.0.0/commit")
                .header("Authorization", "Bearer alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
}

// ---- Slice 11: GET package record ----

#[tokio::test]
async fn get_nonexistent_package_returns_404() {
    let state = test_app();
    let app = router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/packages/acme/nope/1.0.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---- Slice 12: GET version list ----

#[tokio::test]
async fn get_version_list_newest_first() {
    let state = test_app();

    // Insert two versions directly via db
    let record1 = PackageRecord {
        namespace: "acme".into(),
        name: "tool".into(),
        version: "1.0.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0x11)),
        manifest_url: Url::parse("https://blobs.example/m1").unwrap(),
        kind: Some("native".into()),
        description: Some("Tool v1".into()),
        tags: vec![],
        layers: vec![],
        publisher: "alice".into(),
        published_at: "2026-01-01T00:00:00Z".into(),
        signature: None,
        visibility: Visibility::Public,
    };
    let record2 = PackageRecord {
        version: "2.0.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0x22)),
        manifest_url: Url::parse("https://blobs.example/m2").unwrap(),
        published_at: "2026-02-01T00:00:00Z".into(),
        ..record1.clone()
    };

    state.db.put_version(&record1).unwrap();
    state.db.put_version(&record2).unwrap();

    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/packages/acme/tool")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: VersionListResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.versions.len(), 2);
    assert_eq!(resp.versions[0].version, "2.0.0"); // newest first
    assert_eq!(resp.versions[1].version, "1.0.0");
}

// ---- Slice 13: GET search ----

#[tokio::test]
async fn search_returns_matching_packages() {
    let state = test_app();

    let record = PackageRecord {
        namespace: "acme".into(),
        name: "postgres-query".into(),
        version: "1.0.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0x11)),
        manifest_url: Url::parse("https://blobs.example/m1").unwrap(),
        kind: Some("skill".into()),
        description: Some("PostgreSQL queries".into()),
        tags: vec!["database".into()],
        layers: vec![],
        publisher: "alice".into(),
        published_at: "2026-01-01T00:00:00Z".into(),
        signature: None,
        visibility: Visibility::Public,
    };
    state.db.put_version(&record).unwrap();

    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/search?q=postgres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: SearchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].name, "postgres-query");
}

// ---- Slice 14: private visibility on GET ----

#[tokio::test]
async fn private_package_returns_404_without_auth() {
    let state = test_app();

    let record = PackageRecord {
        namespace: "acme-corp".into(),
        name: "secret".into(),
        version: "1.0.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0x11)),
        manifest_url: Url::parse("https://blobs.example/m1").unwrap(),
        kind: Some("native".into()),
        description: Some("Secret package".into()),
        tags: vec![],
        layers: vec![],
        publisher: "acme-corp".into(),
        published_at: "2026-01-01T00:00:00Z".into(),
        signature: None,
        visibility: Visibility::Private,
    };
    state.db.put_version(&record).unwrap();

    // Unauthenticated
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/packages/acme-corp/secret/1.0.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Authenticated as the right namespace
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/packages/acme-corp/secret/1.0.0")
                .header("Authorization", "Bearer acme-corp")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ---- Slice 15: public depends on private rejection ----

#[tokio::test]
async fn public_package_depending_on_private_is_rejected() {
    let state = test_app();

    // First, create a private package
    let private_record = PackageRecord {
        namespace: "acme".into(),
        name: "internal-lib".into(),
        version: "1.0.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0x11)),
        manifest_url: Url::parse("https://blobs.example/m1").unwrap(),
        kind: Some("native".into()),
        description: Some("Private lib".into()),
        tags: vec![],
        layers: vec![],
        publisher: "acme".into(),
        published_at: "2026-01-01T00:00:00Z".into(),
        signature: None,
        visibility: Visibility::Private,
    };
    state.db.put_version(&private_record).unwrap();

    let diff_id = DiffId(test_hash(0xbb));
    let blob_id = BlobId(test_hash(0xcc));

    // Create a manifest that depends on the private package
    let manifest_toml = format!(
        r#"schema = 1

[package]
namespace = "acme"
name = "public-pkg"
version = "1.0.0"
kind = "native"
description = "Public package"

[[layer]]
diff_id = "{diff_id}"
size = 200

[[dependency]]
ref = "acme/internal-lib"
version = "*"
"#
    );

    let manifest_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        manifest_toml.as_bytes(),
    );

    let req_body = serde_json::json!({
        "manifest_blob_id": ManifestHash(test_hash(0xdd)).to_string(),
        "manifest": manifest_b64,
        "layers": [{
            "diff_id": diff_id.to_string(),
            "blob_id": blob_id.to_string(),
            "size_compressed": 100,
            "size_uncompressed": 200
        }],
        "visibility": "public"
    });

    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages/acme/public-pkg/1.0.0")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer acme")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
