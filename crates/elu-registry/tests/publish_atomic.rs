use elu_registry::db::SqliteRegistryDb;
use elu_registry::types::*;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use url::Url;

fn test_hash(b: u8) -> Hash {
    Hash::new(HashAlgo::Sha256, [b; 32])
}

#[test]
fn uncommitted_publish_is_invisible() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    let manifest_blob_id = ManifestHash(test_hash(0xaa));
    let layers = vec![PublishLayerRecord {
        diff_id: DiffId(test_hash(0xbb)),
        blob_id: BlobId(test_hash(0xcc)),
        size_compressed: 100,
        size_uncompressed: 200,
    }];

    db.put_publish_session(
        "sess-1",
        "acme",
        "widget",
        "1.0.0",
        &manifest_blob_id,
        b"manifest data",
        &layers,
        "alice",
        Visibility::Public,
        "2026-03-20T14:22:11Z",
    )
    .unwrap();

    // Version should NOT be visible yet
    let result = db.get_version("acme", "widget", "1.0.0");
    assert!(result.is_err(), "uncommitted version should not be visible");

    let list = db.list_versions("acme", "widget");
    assert!(list.is_err(), "uncommitted version should not appear in list");
}

#[test]
fn commit_makes_version_visible() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    let manifest_blob_id = ManifestHash(test_hash(0xaa));
    let blob_id = BlobId(test_hash(0xcc));
    let layers = vec![PublishLayerRecord {
        diff_id: DiffId(test_hash(0xbb)),
        blob_id: blob_id.clone(),
        size_compressed: 100,
        size_uncompressed: 200,
    }];

    db.put_publish_session(
        "sess-1",
        "acme",
        "widget",
        "1.0.0",
        &manifest_blob_id,
        b"manifest data",
        &layers,
        "alice",
        Visibility::Public,
        "2026-03-20T14:22:11Z",
    )
    .unwrap();

    let manifest_url = Url::parse("https://blobs.example/manifests/aa").unwrap();
    let blob_url = Url::parse("https://blobs.example/blobs/cc").unwrap();

    let record = db
        .commit_version("sess-1", &manifest_url, &[(blob_id, blob_url.clone())])
        .unwrap();

    assert_eq!(record.namespace, "acme");
    assert_eq!(record.name, "widget");
    assert_eq!(record.version, "1.0.0");
    assert_eq!(record.layers.len(), 1);
    assert_eq!(record.layers[0].url, blob_url);

    // Now it should be visible via get
    let fetched = db.get_version("acme", "widget", "1.0.0").unwrap();
    assert_eq!(fetched.manifest_blob_id, manifest_blob_id);
}
