use std::io::Write;

use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::hasher::Hasher;
use elu_store::store::Store;

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

fn sha256(data: &[u8]) -> elu_store::hash::Hash {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

#[test]
fn put_blob_plain_tar() {
    let (_dir, store) = test_store();
    let tar_bytes = make_tar("hello.txt", b"hello world");
    let expected_hash = sha256(&tar_bytes);

    let result = store.put_blob(&mut &tar_bytes[..]).unwrap();

    // For plain tar, blob_id and diff_id have the same hash (identity decompression)
    assert_eq!(result.blob_id.0, expected_hash);
    assert_eq!(result.diff_id.0, expected_hash);
    assert_eq!(result.stored_bytes, tar_bytes.len() as u64);
    assert_eq!(result.diff_bytes, tar_bytes.len() as u64);

    // Verify it's in the store
    assert!(store.has(&result.blob_id).unwrap());
}

#[test]
fn put_blob_gzip() {
    let (_dir, store) = test_store();
    let tar_bytes = make_tar("test.txt", b"gzip test data");

    // Compress with gzip
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    let gz_bytes = encoder.finish().unwrap();

    let expected_blob_hash = sha256(&gz_bytes);
    let expected_diff_hash = sha256(&tar_bytes);

    let result = store.put_blob(&mut &gz_bytes[..]).unwrap();

    assert_eq!(result.blob_id.0, expected_blob_hash);
    assert_eq!(result.diff_id.0, expected_diff_hash);
    assert_eq!(result.stored_bytes, gz_bytes.len() as u64);
    assert_eq!(result.diff_bytes, tar_bytes.len() as u64);
    assert!(store.has(&result.blob_id).unwrap());
}

#[test]
fn put_blob_zstd() {
    let (_dir, store) = test_store();
    let tar_bytes = make_tar("test.txt", b"zstd test data");

    // Compress with zstd
    let zstd_bytes = zstd::encode_all(&tar_bytes[..], 3).unwrap();

    let expected_blob_hash = sha256(&zstd_bytes);
    let expected_diff_hash = sha256(&tar_bytes);

    let result = store.put_blob(&mut &zstd_bytes[..]).unwrap();

    assert_eq!(result.blob_id.0, expected_blob_hash);
    assert_eq!(result.diff_id.0, expected_diff_hash);
    assert_eq!(result.stored_bytes, zstd_bytes.len() as u64);
    assert_eq!(result.diff_bytes, tar_bytes.len() as u64);
    assert!(store.has(&result.blob_id).unwrap());
}

#[test]
fn put_blob_resolve_diff() {
    let (_dir, store) = test_store();
    let tar_bytes = make_tar("resolve.txt", b"resolve test");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    let gz_bytes = encoder.finish().unwrap();

    let result = store.put_blob(&mut &gz_bytes[..]).unwrap();

    // resolve_diff should return the blob_id
    let resolved = store.resolve_diff(&result.diff_id).unwrap().unwrap();
    assert_eq!(resolved, result.blob_id);
    assert!(store.has_diff(&result.diff_id).unwrap());
}

#[test]
fn put_blob_dedup() {
    let (_dir, store) = test_store();
    let tar_bytes = make_tar("dedup.txt", b"same content");
    let mut enc1 = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc1.write_all(&tar_bytes).unwrap();
    let gz_bytes = enc1.finish().unwrap();

    let r1 = store.put_blob(&mut &gz_bytes[..]).unwrap();
    let r2 = store.put_blob(&mut &gz_bytes[..]).unwrap();
    assert_eq!(r1.blob_id, r2.blob_id);
    assert_eq!(r1.diff_id, r2.diff_id);
}

/// Regression net for the streaming-put_blob refactor (cx WKIW.fqtS).
/// 8 MiB compressed input — large enough that an O(blob-size) buffer
/// would be visible in test-allocator metrics, small enough to keep
/// the test fast. Verifies that put_blob still computes the right
/// blob_id/diff_id pair after the refactor and that the stored bytes
/// can be read back identically.
#[test]
fn put_blob_streams_large_blob_correctly() {
    let (_dir, store) = test_store();
    // ~8 MiB of repeating data; gzips well to a few hundred KiB.
    let payload = vec![0xa5u8; 8 * 1024 * 1024];
    let tar_bytes = make_tar("big.bin", &payload);
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    let gz_bytes = encoder.finish().unwrap();

    let expected_blob_hash = sha256(&gz_bytes);
    let expected_diff_hash = sha256(&tar_bytes);

    let result = store.put_blob(&mut &gz_bytes[..]).unwrap();
    assert_eq!(result.blob_id.0, expected_blob_hash);
    assert_eq!(result.diff_id.0, expected_diff_hash);
    assert_eq!(result.stored_bytes, gz_bytes.len() as u64);
    assert_eq!(result.diff_bytes, tar_bytes.len() as u64);

    let stored = store.get(&result.blob_id).unwrap().unwrap();
    assert_eq!(&stored[..], &gz_bytes[..], "stored bytes must round-trip");
}

#[test]
fn get_blob_after_put() {
    let (_dir, store) = test_store();
    let tar_bytes = make_tar("get.txt", b"get test");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    let gz_bytes = encoder.finish().unwrap();

    let result = store.put_blob(&mut &gz_bytes[..]).unwrap();

    // get() returns the raw (compressed) bytes
    let data = store.get(&result.blob_id).unwrap().unwrap();
    assert_eq!(&data[..], &gz_bytes[..]);
}
