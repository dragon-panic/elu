use elu_registry::types::*;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use url::Url;

fn test_hash(hex_byte: u8) -> Hash {
    Hash::new(HashAlgo::Sha256, [hex_byte; 32])
}

#[test]
fn package_record_json_roundtrip() {
    let record = PackageRecord {
        namespace: "ox-community".into(),
        name: "postgres-query".into(),
        version: "0.3.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0xaa)),
        manifest_url: Url::parse("https://blobs.example/manifests/aa").unwrap(),
        kind: Some("ox-skill".into()),
        description: Some("PostgreSQL query skill".into()),
        tags: vec!["database".into(), "postgresql".into()],
        layers: vec![LayerRecord {
            diff_id: DiffId(test_hash(0xbb)),
            blob_id: BlobId(test_hash(0xcc)),
            url: Url::parse("https://blobs.example/blobs/cc").unwrap(),
            size_compressed: 4123,
            size_uncompressed: 18432,
        }],
        publisher: "ox-community".into(),
        published_at: "2026-03-20T14:22:11Z".into(),
        signature: None,
        visibility: Visibility::Public,
    };

    let json = serde_json::to_string(&record).unwrap();
    let deserialized: PackageRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record, deserialized);
}

#[test]
fn publish_request_json_roundtrip() {
    let req = PublishRequest {
        manifest_blob_id: ManifestHash(test_hash(0x11)),
        manifest: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"manifest bytes",
        ),
        layers: vec![PublishLayerRecord {
            diff_id: DiffId(test_hash(0x22)),
            blob_id: BlobId(test_hash(0x33)),
            size_compressed: 100,
            size_uncompressed: 200,
        }],
        visibility: Some(Visibility::Private),
    };

    let json = serde_json::to_string(&req).unwrap();
    let deserialized: PublishRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.manifest_blob_id, deserialized.manifest_blob_id);
    assert_eq!(req.manifest, deserialized.manifest);
    assert_eq!(req.layers, deserialized.layers);
}

#[test]
fn publish_response_json_roundtrip() {
    let resp = PublishResponse {
        session_id: "sess-123".into(),
        upload_urls: vec![UploadUrl {
            blob_id: BlobId(test_hash(0x44)),
            upload_url: Url::parse("https://blobs.example/upload/44").unwrap(),
        }],
    };

    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: PublishResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.session_id, deserialized.session_id);
    assert_eq!(resp.upload_urls.len(), 1);
    assert_eq!(resp.upload_urls[0], deserialized.upload_urls[0]);
}

#[test]
fn visibility_serializes_lowercase() {
    assert_eq!(
        serde_json::to_string(&Visibility::Public).unwrap(),
        "\"public\""
    );
    assert_eq!(
        serde_json::to_string(&Visibility::Private).unwrap(),
        "\"private\""
    );
    assert_eq!(
        serde_json::from_str::<Visibility>("\"public\"").unwrap(),
        Visibility::Public
    );
    assert_eq!(
        serde_json::from_str::<Visibility>("\"private\"").unwrap(),
        Visibility::Private
    );
}

#[test]
fn version_list_response_json_roundtrip() {
    let resp = VersionListResponse {
        namespace: "acme".into(),
        name: "tool".into(),
        versions: vec![
            VersionEntry {
                version: "1.2.0".into(),
                published_at: "2026-03-20T14:22:11Z".into(),
                kind: Some("native".into()),
            },
            VersionEntry {
                version: "1.1.0".into(),
                published_at: "2026-03-10T10:00:00Z".into(),
                kind: Some("native".into()),
            },
        ],
    };

    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: VersionListResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.namespace, deserialized.namespace);
    assert_eq!(resp.versions.len(), 2);
}

#[test]
fn search_response_json_roundtrip() {
    let resp = SearchResponse {
        results: vec![SearchResult {
            namespace: "ox".into(),
            name: "postgres".into(),
            version: "1.0.0".into(),
            kind: Some("ox-skill".into()),
            description: Some("Postgres".into()),
            tags: vec!["db".into()],
            published_at: "2026-01-01T00:00:00Z".into(),
        }],
    };

    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: SearchResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.results.len(), deserialized.results.len());
}
