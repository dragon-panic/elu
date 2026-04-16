use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::store::{RefFilter, Store};

fn test_store() -> (tempfile::TempDir, FsStore) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = camino::Utf8Path::from_path(dir.path()).unwrap();
    let store = FsStore::init_with_fsync(root, FsyncMode::Never).unwrap();
    (dir, store)
}

#[test]
fn put_ref_and_get_ref_roundtrip() {
    let (_dir, store) = test_store();
    let manifest = br#"{"test": "ref-roundtrip"}"#;
    let hash = store.put_manifest(manifest).unwrap();

    store.put_ref("default", "my-pkg", "1.0.0", &hash).unwrap();
    let retrieved = store.get_ref("default", "my-pkg", "1.0.0").unwrap();
    assert_eq!(retrieved, Some(hash));
}

#[test]
fn get_ref_not_found() {
    let (_dir, store) = test_store();
    let result = store.get_ref("default", "no-pkg", "1.0.0").unwrap();
    assert!(result.is_none());
}

#[test]
fn put_ref_idempotent_same_hash() {
    let (_dir, store) = test_store();
    let manifest = br#"{"test": "idempotent"}"#;
    let hash = store.put_manifest(manifest).unwrap();

    store.put_ref("default", "pkg", "1.0.0", &hash).unwrap();
    // Same hash again should succeed
    store.put_ref("default", "pkg", "1.0.0", &hash).unwrap();
}

#[test]
fn put_ref_conflict_different_hash() {
    let (_dir, store) = test_store();
    let m1 = br#"{"test": "first"}"#;
    let m2 = br#"{"test": "second"}"#;
    let h1 = store.put_manifest(m1).unwrap();
    let h2 = store.put_manifest(m2).unwrap();

    store.put_ref("default", "pkg", "1.0.0", &h1).unwrap();
    let result = store.put_ref("default", "pkg", "1.0.0", &h2);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("ref conflict"));
}

#[test]
fn list_refs_all() {
    let (_dir, store) = test_store();
    let m1 = br#"{"name": "alpha"}"#;
    let m2 = br#"{"name": "beta"}"#;
    let h1 = store.put_manifest(m1).unwrap();
    let h2 = store.put_manifest(m2).unwrap();

    store.put_ref("default", "alpha", "1.0.0", &h1).unwrap();
    store.put_ref("default", "beta", "2.0.0", &h2).unwrap();

    let refs = store.list_refs(RefFilter::default()).unwrap();
    assert_eq!(refs.len(), 2);
}

#[test]
fn list_refs_with_namespace_filter() {
    let (_dir, store) = test_store();
    let m1 = br#"{"ns": "a"}"#;
    let m2 = br#"{"ns": "b"}"#;
    let h1 = store.put_manifest(m1).unwrap();
    let h2 = store.put_manifest(m2).unwrap();

    store.put_ref("ns-a", "pkg", "1.0.0", &h1).unwrap();
    store.put_ref("ns-b", "pkg", "1.0.0", &h2).unwrap();

    let refs = store
        .list_refs(RefFilter {
            namespace: Some("ns-a".to_string()),
            name: None,
        })
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].namespace, "ns-a");
}

#[test]
fn list_refs_with_name_filter() {
    let (_dir, store) = test_store();
    let m1 = br#"{"pkg": "a"}"#;
    let m2 = br#"{"pkg": "b"}"#;
    let h1 = store.put_manifest(m1).unwrap();
    let h2 = store.put_manifest(m2).unwrap();

    store.put_ref("default", "alpha", "1.0.0", &h1).unwrap();
    store.put_ref("default", "beta", "1.0.0", &h2).unwrap();

    let refs = store
        .list_refs(RefFilter {
            namespace: None,
            name: Some("alpha".to_string()),
        })
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].name, "alpha");
}

#[test]
fn ref_component_validation_rejects_slash() {
    let (_dir, store) = test_store();
    let m = br#"{"v": 1}"#;
    let h = store.put_manifest(m).unwrap();
    assert!(store.put_ref("bad/ns", "pkg", "1.0.0", &h).is_err());
}

#[test]
fn ref_component_validation_rejects_backslash() {
    let (_dir, store) = test_store();
    let m = br#"{"v": 2}"#;
    let h = store.put_manifest(m).unwrap();
    assert!(store.put_ref("ns", "bad\\name", "1.0.0", &h).is_err());
}

#[test]
fn ref_component_validation_rejects_dotdot() {
    let (_dir, store) = test_store();
    let m = br#"{"v": 3}"#;
    let h = store.put_manifest(m).unwrap();
    assert!(store.put_ref("ns", "pkg", "..", &h).is_err());
}

#[test]
fn ref_component_validation_rejects_control_chars() {
    let (_dir, store) = test_store();
    let m = br#"{"v": 4}"#;
    let h = store.put_manifest(m).unwrap();
    assert!(store.put_ref("ns", "pkg\x00", "1.0.0", &h).is_err());
}

#[test]
fn ref_component_validation_rejects_empty() {
    let (_dir, store) = test_store();
    let m = br#"{"v": 5}"#;
    let h = store.put_manifest(m).unwrap();
    assert!(store.put_ref("", "pkg", "1.0.0", &h).is_err());
}

#[test]
fn multiple_versions_same_package() {
    let (_dir, store) = test_store();
    let m1 = br#"{"ver": "1"}"#;
    let m2 = br#"{"ver": "2"}"#;
    let h1 = store.put_manifest(m1).unwrap();
    let h2 = store.put_manifest(m2).unwrap();

    store.put_ref("default", "pkg", "1.0.0", &h1).unwrap();
    store.put_ref("default", "pkg", "2.0.0", &h2).unwrap();

    assert_eq!(store.get_ref("default", "pkg", "1.0.0").unwrap(), Some(h1));
    assert_eq!(store.get_ref("default", "pkg", "2.0.0").unwrap(), Some(h2));
}
