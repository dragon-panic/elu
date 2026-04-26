use std::io::Write;

use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::hash::BlobId;
use elu_store::error::StoreError;
use elu_store::store::{FsckError, Store};

fn test_store() -> (tempfile::TempDir, FsStore) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = camino::Utf8Path::from_path(dir.path()).unwrap();
    let store = FsStore::init_with_fsync(root, FsyncMode::Never).unwrap();
    (dir, store)
}

fn make_tar(filename: &str, content: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_path(filename).unwrap();
    header.set_size(content.len() as u64);
    header.set_cksum();
    builder.append(&header, content).unwrap();
    builder.into_inner().unwrap()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn fsck_clean_store() {
    let (_dir, store) = test_store();
    let manifest = br#"{"clean": true}"#;
    let hash = store.put_manifest(manifest).unwrap();
    store.put_ref("default", "pkg", "1.0.0", &hash).unwrap();

    let errors = store.fsck().unwrap();
    assert!(errors.is_empty());
}

#[test]
fn fsck_detects_corrupted_object() {
    let (dir, store) = test_store();
    let manifest = br#"{"will": "corrupt"}"#;
    let hash = store.put_manifest(manifest).unwrap();

    // Corrupt the object on disk
    let blob_id = BlobId(hash.0.clone());
    let h = &blob_id.0;
    let obj_path = dir
        .path()
        .join("objects")
        .join(h.algo().as_str())
        .join(h.prefix())
        .join(h.rest());
    // Flip a byte
    let mut data = std::fs::read(&obj_path).unwrap();
    data[0] ^= 0xff;
    std::fs::write(&obj_path, &data).unwrap();

    let errors = store.fsck().unwrap();
    assert!(!errors.is_empty());
    assert!(matches!(&errors[0], FsckError::HashMismatch { .. }));
}

#[test]
fn fsck_detects_orphaned_diff() {
    let (dir, store) = test_store();
    let tar_bytes = make_tar("orphan.txt", b"will have orphaned diff");
    let gz_bytes = gzip(&tar_bytes);
    let result = store.put_blob(&mut &gz_bytes[..]).unwrap();

    // Delete the blob from objects/ but leave the diffs/ entry
    let h = &result.blob_id.0;
    let obj_path = dir
        .path()
        .join("objects")
        .join(h.algo().as_str())
        .join(h.prefix())
        .join(h.rest());
    std::fs::remove_file(&obj_path).unwrap();

    let errors = store.fsck().unwrap();
    let has_orphaned = errors.iter().any(|e| matches!(e, FsckError::OrphanedDiff { .. }));
    assert!(has_orphaned);
}

#[test]
fn fsck_detects_broken_ref() {
    let (dir, store) = test_store();
    let manifest = br#"{"ref": "broken"}"#;
    let hash = store.put_manifest(manifest).unwrap();
    store.put_ref("default", "pkg", "1.0.0", &hash).unwrap();

    // Delete the manifest blob
    let blob_id = BlobId(hash.0.clone());
    let h = &blob_id.0;
    let obj_path = dir
        .path()
        .join("objects")
        .join(h.algo().as_str())
        .join(h.prefix())
        .join(h.rest());
    std::fs::remove_file(&obj_path).unwrap();

    let errors = store.fsck().unwrap();
    let has_broken_ref = errors.iter().any(|e| matches!(e, FsckError::BrokenRef { .. }));
    assert!(has_broken_ref);
}

#[test]
fn fsck_repair_removes_orphaned_diff() {
    let (dir, store) = test_store();
    let tar_bytes = make_tar("orphan.txt", b"will have orphaned diff");
    let gz_bytes = gzip(&tar_bytes);
    let result = store.put_blob(&mut &gz_bytes[..]).unwrap();
    let h = &result.blob_id.0;
    let obj_path = dir
        .path()
        .join("objects")
        .join(h.algo().as_str())
        .join(h.prefix())
        .join(h.rest());
    std::fs::remove_file(&obj_path).unwrap();

    let report = store.fsck_repair().unwrap();
    assert_eq!(report.orphaned_diffs_removed, 1);
    assert!(store.fsck().unwrap().is_empty(), "fsck should be clean post-repair");
}

#[test]
fn fsck_repair_removes_broken_ref() {
    let (dir, store) = test_store();
    let manifest = br#"{"ref": "broken"}"#;
    let hash = store.put_manifest(manifest).unwrap();
    store.put_ref("default", "pkg", "1.0.0", &hash).unwrap();
    let blob_id = BlobId(hash.0.clone());
    let h = &blob_id.0;
    let obj_path = dir
        .path()
        .join("objects")
        .join(h.algo().as_str())
        .join(h.prefix())
        .join(h.rest());
    std::fs::remove_file(&obj_path).unwrap();

    let report = store.fsck_repair().unwrap();
    assert_eq!(report.broken_refs_removed, 1);
    assert!(store.fsck().unwrap().is_empty(), "fsck should be clean post-repair");
}

#[test]
fn fsck_repair_returns_unrepairable_on_hash_mismatch() {
    let (dir, store) = test_store();
    let manifest = br#"{"will": "corrupt"}"#;
    let hash = store.put_manifest(manifest).unwrap();
    let blob_id = BlobId(hash.0.clone());
    let h = &blob_id.0;
    let obj_path = dir
        .path()
        .join("objects")
        .join(h.algo().as_str())
        .join(h.prefix())
        .join(h.rest());
    let mut data = std::fs::read(&obj_path).unwrap();
    data[0] ^= 0xff;
    std::fs::write(&obj_path, &data).unwrap();

    let err = store.fsck_repair().unwrap_err();
    assert!(
        matches!(err, StoreError::FsckUnrepairable(n) if n >= 1),
        "expected FsckUnrepairable, got: {err:?}",
    );
}
