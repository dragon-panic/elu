use std::io::Cursor;
use std::path::Path;

use elu_store::hash::DiffId;
use elu_store::store::Store;

use crate::error::ImportError;

/// Result of packing a directory into a tar layer and storing it.
pub struct PackedLayer {
    pub diff_id: DiffId,
    pub size: u64,
}

/// Pack a staging directory into a tar archive, store it as a blob,
/// and return the layer identity (diff_id) and uncompressed size.
pub fn pack_dir(staging: &Path, store: &dyn Store) -> Result<PackedLayer, ImportError> {
    let tar_bytes = build_tar(staging)?;
    let size = tar_bytes.len() as u64;
    let mut cursor = Cursor::new(tar_bytes);
    let put = store.put_blob(&mut cursor)?;
    Ok(PackedLayer {
        diff_id: put.diff_id,
        size,
    })
}

/// Build a tar archive from the contents of `dir`.
fn build_tar(dir: &Path) -> Result<Vec<u8>, ImportError> {
    let buf = Vec::new();
    let mut ar = tar::Builder::new(buf);
    ar.follow_symlinks(false);
    ar.append_dir_all("", dir)?;
    ar.into_inner().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tar_creates_valid_archive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), b"hello").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/world.txt"), b"world").unwrap();

        let tar_bytes = build_tar(dir.path()).unwrap();
        assert!(!tar_bytes.is_empty());

        // Verify we can read entries back
        let mut archive = tar::Archive::new(Cursor::new(&tar_bytes));
        let entries: Vec<String> = archive
            .entries()
            .unwrap()
            .filter_map(|e| {
                let e = e.unwrap();
                let path = e.path().unwrap().to_string_lossy().to_string();
                if e.header().entry_type().is_file() {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        assert!(entries.contains(&"hello.txt".to_string()));
        assert!(entries.contains(&"sub/world.txt".to_string()));
    }

    #[test]
    fn pack_dir_stores_blob_in_store() {
        let store_dir = tempfile::tempdir().unwrap();
        let store_root = camino::Utf8Path::from_path(store_dir.path()).unwrap();
        let store =
            elu_store::fs_store::FsStore::init_with_fsync(store_root, elu_store::atomic::FsyncMode::Never)
                .unwrap();

        let staging = tempfile::tempdir().unwrap();
        std::fs::write(staging.path().join("file.txt"), b"content").unwrap();

        let packed = pack_dir(staging.path(), &store).unwrap();
        assert!(packed.size > 0);
        assert!(store.has_diff(&packed.diff_id).unwrap());
    }
}
