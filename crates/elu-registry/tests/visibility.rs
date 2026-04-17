use elu_registry::db::SqliteRegistryDb;
use elu_registry::types::*;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use url::Url;

fn test_hash(b: u8) -> Hash {
    Hash::new(HashAlgo::Sha256, [b; 32])
}

fn private_record() -> PackageRecord {
    PackageRecord {
        namespace: "acme-corp".into(),
        name: "internal-tool".into(),
        version: "1.0.0".into(),
        manifest_blob_id: ManifestHash(test_hash(0xaa)),
        manifest_url: Url::parse("https://blobs.example/manifests/aa").unwrap(),
        kind: Some("native".into()),
        description: Some("Internal tool".into()),
        tags: vec![],
        layers: vec![LayerRecord {
            diff_id: DiffId(test_hash(0xbb)),
            blob_id: BlobId(test_hash(0xcc)),
            url: Url::parse("https://blobs.example/blobs/cc").unwrap(),
            size_compressed: 100,
            size_uncompressed: 200,
        }],
        publisher: "acme-corp".into(),
        published_at: "2026-03-20T14:22:11Z".into(),
        signature: None,
        visibility: Visibility::Private,
    }
}

#[test]
fn private_package_visible_to_authenticated_member() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&private_record()).unwrap();

    let result = db.get_version_with_visibility("acme-corp", "internal-tool", "1.0.0", Some("acme-corp"));
    assert!(result.is_ok());
}

#[test]
fn private_package_invisible_to_unauthenticated() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&private_record()).unwrap();

    let result = db.get_version_with_visibility("acme-corp", "internal-tool", "1.0.0", None);
    assert!(result.is_err());
}

#[test]
fn private_package_invisible_to_other_namespace() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&private_record()).unwrap();

    let result = db.get_version_with_visibility("acme-corp", "internal-tool", "1.0.0", Some("other-org"));
    assert!(result.is_err());
}

#[test]
fn private_packages_hidden_from_search() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&private_record()).unwrap();

    // Unauthenticated search
    let results = db.search(&SearchQuery { q: Some("internal".into()), ..Default::default() }, None).unwrap();
    assert!(results.is_empty(), "private packages should not appear in unauthenticated search");

    // Authenticated as the right namespace
    let results = db.search(&SearchQuery { q: Some("internal".into()), ..Default::default() }, Some("acme-corp")).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn private_package_list_versions_hidden_from_unauthenticated() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&private_record()).unwrap();

    let result = db.list_versions_with_visibility("acme-corp", "internal-tool", None);
    assert!(result.is_err(), "should not list private package versions for unauthenticated user");

    let result = db.list_versions_with_visibility("acme-corp", "internal-tool", Some("acme-corp"));
    assert!(result.is_ok());
}
