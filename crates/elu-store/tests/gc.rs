use std::io::Write;

use elu_store::atomic::FsyncMode;
use elu_store::error::StoreError;
use elu_store::fs_store::FsStore;
use elu_store::hash::{DiffId, ManifestHash};
use elu_store::store::{ManifestReader, Store};

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

/// Simple test ManifestReader that parses JSON manifests with the shape:
/// {"layers": ["sha256:..."], "dependencies": ["sha256:..."]}
struct TestManifestReader;

impl ManifestReader for TestManifestReader {
    fn layer_diff_ids(&self, bytes: &[u8]) -> Result<Vec<DiffId>, StoreError> {
        let v: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| StoreError::ManifestRead(e.to_string()))?;
        let mut ids = Vec::new();
        if let Some(layers) = v.get("layers").and_then(|l| l.as_array()) {
            for layer in layers {
                if let Some(s) = layer.as_str() {
                    let id: DiffId = s.parse().map_err(|e: elu_store::hash::HashParseError| {
                        StoreError::ManifestRead(e.to_string())
                    })?;
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    fn dependency_hashes(&self, bytes: &[u8]) -> Result<Vec<ManifestHash>, StoreError> {
        let v: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| StoreError::ManifestRead(e.to_string()))?;
        let mut hashes = Vec::new();
        if let Some(deps) = v.get("dependencies").and_then(|d| d.as_array()) {
            for dep in deps {
                if let Some(s) = dep.as_str() {
                    let h: ManifestHash =
                        s.parse().map_err(|e: elu_store::hash::HashParseError| {
                            StoreError::ManifestRead(e.to_string())
                        })?;
                    hashes.push(h);
                }
            }
        }
        Ok(hashes)
    }
}

#[test]
fn gc_reclaims_unreachable_objects() {
    let (_dir, store) = test_store();

    // Create a layer blob (reachable)
    let tar_bytes = make_tar("reachable.txt", b"reachable");
    let gz_bytes = gzip(&tar_bytes);
    let blob_result = store.put_blob(&mut &gz_bytes[..]).unwrap();

    // Create a manifest referencing the layer
    let manifest = format!(
        r#"{{"layers": ["{}"], "dependencies": []}}"#,
        blob_result.diff_id
    );
    let manifest_hash = store.put_manifest(manifest.as_bytes()).unwrap();

    // Create a ref pointing to the manifest
    store
        .put_ref("default", "pkg", "1.0.0", &manifest_hash)
        .unwrap();

    // Create an unreachable blob
    let orphan_tar = make_tar("orphan.txt", b"orphan data");
    let orphan_gz = gzip(&orphan_tar);
    let orphan_result = store.put_blob(&mut &orphan_gz[..]).unwrap();
    assert!(store.has(&orphan_result.blob_id).unwrap());

    // Run GC
    let stats = store.gc(&TestManifestReader).unwrap();

    // Reachable objects should survive
    assert!(store.has(&blob_result.blob_id).unwrap());
    assert!(store.has_diff(&blob_result.diff_id).unwrap());
    let manifest_blob = elu_store::hash::BlobId(manifest_hash.0.clone());
    assert!(store.has(&manifest_blob).unwrap());

    // Unreachable objects should be gone
    assert!(!store.has(&orphan_result.blob_id).unwrap());
    assert!(stats.objects_removed >= 1);
}

#[test]
fn gc_with_dependencies() {
    let (_dir, store) = test_store();

    // Create a dependency layer + manifest
    let dep_tar = make_tar("dep.txt", b"dependency");
    let dep_gz = gzip(&dep_tar);
    let dep_blob = store.put_blob(&mut &dep_gz[..]).unwrap();

    let dep_manifest = format!(
        r#"{{"layers": ["{}"], "dependencies": []}}"#,
        dep_blob.diff_id
    );
    let dep_hash = store.put_manifest(dep_manifest.as_bytes()).unwrap();

    // Create a top-level manifest that depends on dep
    let top_tar = make_tar("top.txt", b"top level");
    let top_gz = gzip(&top_tar);
    let top_blob = store.put_blob(&mut &top_gz[..]).unwrap();

    let top_manifest = format!(
        r#"{{"layers": ["{}"], "dependencies": ["{}"]}}"#,
        top_blob.diff_id, dep_hash
    );
    let top_hash = store.put_manifest(top_manifest.as_bytes()).unwrap();

    // Only ref the top manifest
    store
        .put_ref("default", "top", "1.0.0", &top_hash)
        .unwrap();

    // GC should keep both manifests and both layers
    let stats = store.gc(&TestManifestReader).unwrap();
    assert_eq!(stats.objects_removed, 0);
    assert!(store.has(&dep_blob.blob_id).unwrap());
    assert!(store.has(&top_blob.blob_id).unwrap());
}

#[test]
fn gc_cleans_stale_tmp_files() {
    let (dir, store) = test_store();

    // Create a file in tmp/ and set its mtime to 2 days ago
    let tmp_path = dir.path().join("tmp").join("stale-file");
    std::fs::write(&tmp_path, b"stale").unwrap();
    let two_days_ago =
        std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 24 * 60 * 60);
    filetime::set_file_mtime(
        &tmp_path,
        filetime::FileTime::from_system_time(two_days_ago),
    )
    .unwrap();

    // Create a fresh tmp file that should NOT be cleaned
    let fresh_path = dir.path().join("tmp").join("fresh-file");
    std::fs::write(&fresh_path, b"fresh").unwrap();

    let stats = store.gc(&TestManifestReader).unwrap();
    assert_eq!(stats.tmp_removed, 1);
    assert!(!tmp_path.exists());
    assert!(fresh_path.exists());
}

#[test]
fn gc_cleans_orphaned_diffs() {
    let (_dir, store) = test_store();

    // Put a blob (creates both objects/ and diffs/ entries)
    let tar_bytes = make_tar("orphan-diff.txt", b"will be orphaned");
    let gz_bytes = gzip(&tar_bytes);
    let result = store.put_blob(&mut &gz_bytes[..]).unwrap();
    assert!(store.has_diff(&result.diff_id).unwrap());

    // No refs — everything is garbage
    let stats = store.gc(&TestManifestReader).unwrap();
    assert!(!store.has(&result.blob_id).unwrap());
    assert!(!store.has_diff(&result.diff_id).unwrap());
    assert!(stats.objects_removed >= 1);
    assert!(stats.diffs_removed >= 1);
}
