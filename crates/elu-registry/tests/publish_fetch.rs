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
        description: Some("A widget package".into()),
        tags: vec!["util".into()],
        layers: vec![LayerRecord {
            diff_id: DiffId(test_hash(0xbb)),
            blob_id: BlobId(test_hash(0xcc)),
            url: Url::parse("https://blobs.example/blobs/cc").unwrap(),
            size_compressed: 1000,
            size_uncompressed: 2000,
        }],
        publisher: "alice".into(),
        published_at: "2026-03-20T14:22:11Z".into(),
        signature: None,
        visibility: Visibility::Public,
    }
}

#[test]
fn put_and_get_version() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    let record = sample_record();

    db.put_version(&record).unwrap();

    let fetched = db.get_version("acme", "widget", "1.0.0").unwrap();
    assert_eq!(fetched, record);
}

#[test]
fn get_nonexistent_version_returns_error() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    let result = db.get_version("acme", "widget", "1.0.0");
    assert!(result.is_err());
}
