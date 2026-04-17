use elu_registry::db::SqliteRegistryDb;
use elu_registry::types::*;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
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
        manifest_url: Url::parse("https://blobs.example/manifests/aa").unwrap(),
        kind: Some("native".into()),
        description: Some("A widget".into()),
        tags: vec![],
        layers: vec![LayerRecord {
            diff_id: DiffId(test_hash(0xbb)),
            blob_id: BlobId(test_hash(0xcc)),
            url: Url::parse("https://blobs.example/blobs/cc").unwrap(),
            size_compressed: 100,
            size_uncompressed: 200,
        }],
        publisher: "alice".into(),
        published_at: "2026-03-20T14:22:11Z".into(),
        signature: None,
        visibility: Visibility::Public,
    }
}

#[test]
fn duplicate_version_rejected() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    let record = sample_record();

    db.put_version(&record).unwrap();

    // Attempt to publish same version again
    let result = db.put_version(&record);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("already exists"),
        "expected VersionExists error, got: {err}"
    );
}

#[test]
fn duplicate_session_rejected_if_version_exists() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    let record = sample_record();
    db.put_version(&record).unwrap();

    let result = db.put_publish_session(
        "sess-2",
        "acme",
        "widget",
        "1.0.0",
        &ManifestHash(test_hash(0xdd)),
        b"other manifest",
        &[],
        "bob",
        Visibility::Public,
        "2026-03-21T00:00:00Z",
    );

    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("already exists"),
        "should reject session for existing version"
    );
}
