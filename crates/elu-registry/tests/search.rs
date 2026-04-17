use elu_registry::db::SqliteRegistryDb;
use elu_registry::types::*;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use url::Url;

fn test_hash(b: u8) -> Hash {
    Hash::new(HashAlgo::Sha256, [b; 32])
}

fn make_record(ns: &str, name: &str, version: &str, kind: &str, desc: &str, tags: Vec<&str>, ts: &str) -> PackageRecord {
    PackageRecord {
        namespace: ns.into(),
        name: name.into(),
        version: version.into(),
        manifest_blob_id: ManifestHash(test_hash(version.as_bytes()[0])),
        manifest_url: Url::parse(&format!("https://blobs.example/manifests/{name}")).unwrap(),
        kind: Some(kind.into()),
        description: Some(desc.into()),
        tags: tags.into_iter().map(String::from).collect(),
        layers: vec![LayerRecord {
            diff_id: DiffId(test_hash(0x01)),
            blob_id: BlobId(test_hash(0x02)),
            url: Url::parse("https://blobs.example/blobs/01").unwrap(),
            size_compressed: 100,
            size_uncompressed: 200,
        }],
        publisher: "test".into(),
        published_at: ts.into(),
        signature: None,
        visibility: Visibility::Public,
    }
}

#[test]
fn search_by_name() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&make_record("acme", "postgres-query", "1.0.0", "skill", "PostgreSQL queries", vec!["db"], "2026-01-01T00:00:00Z")).unwrap();
    db.put_version(&make_record("acme", "redis-cache", "1.0.0", "skill", "Redis caching", vec!["cache"], "2026-01-02T00:00:00Z")).unwrap();

    let results = db.search(&SearchQuery { q: Some("postgres".into()), ..Default::default() }, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "postgres-query");
}

#[test]
fn search_by_kind() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&make_record("acme", "pkg1", "1.0.0", "native", "native pkg", vec![], "2026-01-01T00:00:00Z")).unwrap();
    db.put_version(&make_record("acme", "pkg2", "1.0.0", "skill", "skill pkg", vec![], "2026-01-02T00:00:00Z")).unwrap();

    let results = db.search(&SearchQuery { kind: Some("skill".into()), ..Default::default() }, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "pkg2");
}

#[test]
fn search_by_tag() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&make_record("acme", "pkg1", "1.0.0", "native", "p1", vec!["database", "sql"], "2026-01-01T00:00:00Z")).unwrap();
    db.put_version(&make_record("acme", "pkg2", "1.0.0", "native", "p2", vec!["cache"], "2026-01-02T00:00:00Z")).unwrap();

    let results = db.search(&SearchQuery { tag: Some("database".into()), ..Default::default() }, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "pkg1");
}

#[test]
fn search_by_namespace() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&make_record("acme", "pkg1", "1.0.0", "native", "p1", vec![], "2026-01-01T00:00:00Z")).unwrap();
    db.put_version(&make_record("other", "pkg2", "1.0.0", "native", "p2", vec![], "2026-01-02T00:00:00Z")).unwrap();

    let results = db.search(&SearchQuery { namespace: Some("acme".into()), ..Default::default() }, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].namespace, "acme");
}

#[test]
fn search_returns_latest_version_only() {
    let db = SqliteRegistryDb::open_in_memory().unwrap();
    db.put_version(&make_record("acme", "pkg1", "1.0.0", "native", "p1", vec![], "2026-01-01T00:00:00Z")).unwrap();
    db.put_version(&make_record("acme", "pkg1", "2.0.0", "native", "p1 v2", vec![], "2026-02-01T00:00:00Z")).unwrap();

    let results = db.search(&SearchQuery { q: Some("p1".into()), ..Default::default() }, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].version, "2.0.0");
}
