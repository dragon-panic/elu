use std::fs::File;
use std::io::Read;

use bytes::Bytes;

use crate::error::StoreError;
use crate::hash::{BlobId, DiffId, ManifestHash};

#[derive(Debug)]
pub struct PutBlob {
    pub diff_id: DiffId,
    pub blob_id: BlobId,
    pub stored_bytes: u64,
    pub diff_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RefFilter {
    pub namespace: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefEntry {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub hash: ManifestHash,
}

#[derive(Debug, Clone, Default)]
pub struct GcStats {
    pub objects_removed: u64,
    pub diffs_removed: u64,
    pub tmp_removed: u64,
    pub bytes_freed: u64,
}

#[derive(Debug, Clone)]
pub enum FsckError {
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    OrphanedDiff {
        diff_id: String,
        blob_id: String,
    },
    BrokenRef {
        ref_path: String,
        target: String,
    },
}

pub trait Store {
    fn get(&self, id: &BlobId) -> Result<Option<Bytes>, StoreError>;
    fn open(&self, id: &BlobId) -> Result<Option<File>, StoreError>;
    fn has(&self, id: &BlobId) -> Result<bool, StoreError>;
    fn size(&self, id: &BlobId) -> Result<Option<u64>, StoreError>;

    fn put_blob(&self, bytes: &mut dyn Read) -> Result<PutBlob, StoreError>;
    fn resolve_diff(&self, id: &DiffId) -> Result<Option<BlobId>, StoreError>;
    fn has_diff(&self, id: &DiffId) -> Result<bool, StoreError>;

    fn put_manifest(&self, bytes: &[u8]) -> Result<ManifestHash, StoreError>;
    fn get_manifest(&self, id: &ManifestHash) -> Result<Option<Bytes>, StoreError>;

    fn put_ref(
        &self,
        ns: &str,
        name: &str,
        version: &str,
        hash: &ManifestHash,
    ) -> Result<(), StoreError>;
    fn get_ref(
        &self,
        ns: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<ManifestHash>, StoreError>;
    fn list_refs(&self, filter: RefFilter) -> Result<Vec<RefEntry>, StoreError>;

    fn gc(&self, reader: &dyn ManifestReader) -> Result<GcStats, StoreError>;
    fn fsck(&self) -> Result<Vec<FsckError>, StoreError>;
}

pub trait ManifestReader {
    fn layer_diff_ids(&self, bytes: &[u8]) -> Result<Vec<DiffId>, StoreError>;
    fn dependency_hashes(&self, bytes: &[u8]) -> Result<Vec<ManifestHash>, StoreError>;
}
