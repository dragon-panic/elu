use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::store::Store;

fn test_store() -> (tempfile::TempDir, FsStore) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = camino::Utf8Path::from_path(dir.path()).unwrap();
    let store = FsStore::init_with_fsync(root, FsyncMode::Never).unwrap();
    (dir, store)
}

#[test]
fn init_creates_layout_directories() {
    let (dir, _store) = test_store();
    let root = dir.path();
    assert!(root.join("objects").is_dir());
    assert!(root.join("diffs").is_dir());
    assert!(root.join("manifests").is_dir());
    assert!(root.join("refs").is_dir());
    assert!(root.join("tmp").is_dir());
    assert!(root.join("locks").is_dir());
}

#[test]
fn put_manifest_and_get_roundtrip() {
    let (_dir, store) = test_store();
    let manifest = br#"{"version": 1, "layers": []}"#;
    let hash = store.put_manifest(manifest).unwrap();
    let retrieved = store.get_manifest(&hash).unwrap().expect("should exist");
    assert_eq!(&retrieved[..], manifest);
}

#[test]
fn put_manifest_deduplicates() {
    let (_dir, store) = test_store();
    let manifest = br#"{"version": 1}"#;
    let hash1 = store.put_manifest(manifest).unwrap();
    let hash2 = store.put_manifest(manifest).unwrap();
    assert_eq!(hash1, hash2);
}

#[test]
fn put_manifest_rejects_non_utf8() {
    let (_dir, store) = test_store();
    let bad_bytes: &[u8] = &[0xff, 0xfe, 0x00, 0x01];
    let result = store.put_manifest(bad_bytes);
    assert!(result.is_err());
}

#[test]
fn has_and_size_work() {
    let (_dir, store) = test_store();
    let manifest = br#"{"test": true}"#;
    let hash = store.put_manifest(manifest).unwrap();
    let blob_id = elu_store::hash::BlobId(hash.0.clone());
    assert!(store.has(&blob_id).unwrap());
    assert_eq!(store.size(&blob_id).unwrap(), Some(manifest.len() as u64));
}

#[test]
fn get_nonexistent_returns_none() {
    let (_dir, store) = test_store();
    let hash = elu_store::hash::ManifestHash(
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .unwrap(),
    );
    assert!(store.get_manifest(&hash).unwrap().is_none());
}

#[test]
fn open_existing_store() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = camino::Utf8Path::from_path(dir.path()).unwrap();
    let _store = FsStore::init_with_fsync(root, FsyncMode::Never).unwrap();
    // Now open it
    let store2 = FsStore::open(root).unwrap();
    let manifest = br#"{"opened": true}"#;
    let hash = store2.put_manifest(manifest).unwrap();
    assert!(store2.get_manifest(&hash).unwrap().is_some());
}

#[test]
fn open_nonexistent_fails() {
    let result = FsStore::open("/tmp/does-not-exist-elu-test");
    assert!(result.is_err());
}
