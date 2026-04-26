mod common;

use std::collections::HashSet;

use bytes::Bytes;
use common::{InMemorySource, make_manifest_with_layers, pkgref, synth_diff, vrange};
use elu_resolver::resolve;
use elu_resolver::types::{FetchKind, RootRef};
use elu_store::error::StoreError;
use elu_store::hash::{BlobId, DiffId, ManifestHash};
use elu_store::store::{
    FsckError, GcStats, ManifestReader, PutBlob, RefEntry, RefFilter, Store,
};

/// Minimal Store stub: tracks which blob_ids/diff_ids/manifest_hashes are
/// "present" and answers has/has_diff/get_manifest accordingly. Other methods
/// panic — slice 12 doesn't exercise them.
struct StubStore {
    diffs: HashSet<DiffId>,
    manifests: HashSet<ManifestHash>,
}

impl StubStore {
    fn new() -> Self {
        Self {
            diffs: HashSet::new(),
            manifests: HashSet::new(),
        }
    }
}

impl Store for StubStore {
    fn get(&self, _id: &BlobId) -> Result<Option<Bytes>, StoreError> {
        Ok(None)
    }
    fn open(&self, _id: &BlobId) -> Result<Option<std::fs::File>, StoreError> {
        Ok(None)
    }
    fn has(&self, _id: &BlobId) -> Result<bool, StoreError> {
        Ok(false)
    }
    fn size(&self, _id: &BlobId) -> Result<Option<u64>, StoreError> {
        Ok(None)
    }
    fn put_blob(&self, _bytes: &mut dyn std::io::Read) -> Result<PutBlob, StoreError> {
        unreachable!("not used by resolver")
    }
    fn resolve_diff(&self, _id: &DiffId) -> Result<Option<BlobId>, StoreError> {
        Ok(None)
    }
    fn has_diff(&self, id: &DiffId) -> Result<bool, StoreError> {
        Ok(self.diffs.contains(id))
    }
    fn put_manifest(&self, _bytes: &[u8]) -> Result<ManifestHash, StoreError> {
        unreachable!("not used by resolver")
    }
    fn get_manifest(&self, id: &ManifestHash) -> Result<Option<Bytes>, StoreError> {
        if self.manifests.contains(id) {
            Ok(Some(Bytes::from_static(b"present")))
        } else {
            Ok(None)
        }
    }
    fn put_ref(
        &self,
        _ns: &str,
        _name: &str,
        _version: &str,
        _hash: &ManifestHash,
    ) -> Result<(), StoreError> {
        unreachable!("not used by resolver")
    }
    fn get_ref(
        &self,
        _ns: &str,
        _name: &str,
        _version: &str,
    ) -> Result<Option<ManifestHash>, StoreError> {
        Ok(None)
    }
    fn list_refs(&self, _filter: RefFilter) -> Result<Vec<RefEntry>, StoreError> {
        Ok(vec![])
    }
    fn remove_ref(&self, _ns: &str, _name: &str, _version: &str) -> Result<(), StoreError> {
        unreachable!("not used by resolver")
    }
    fn gc(&self, _reader: &dyn ManifestReader) -> Result<GcStats, StoreError> {
        unreachable!("not used by resolver")
    }
    fn plan_gc(&self, _reader: &dyn ManifestReader) -> Result<elu_store::store::GcPlan, StoreError> {
        unreachable!("not used by resolver")
    }
    fn fsck(&self) -> Result<Vec<FsckError>, StoreError> {
        unreachable!("not used by resolver")
    }
}

/// Slice 12: fetch_plan lists exactly the manifests and layers absent from
/// the local store; entries already present are omitted.
#[tokio::test]
async fn fetch_plan_lists_only_missing_blobs() {
    let mut src = InMemorySource::new();

    let l_present = synth_diff(0xa1);
    let l_missing = synth_diff(0xa2);

    let m = make_manifest_with_layers(
        "acme",
        "thing",
        "1.0.0",
        vec![],
        vec![l_present.clone(), l_missing.clone()],
    );
    let manifest_hash = src.add(m);

    let mut store = StubStore::new();
    store.diffs.insert(l_present.clone());
    // manifest_hash NOT in store → should appear in fetch plan

    let root = RootRef {
        package: pkgref("acme", "thing"),
        version: vrange("^1"),
    };

    let resolution = resolve(&[root], &src, None, Some(&store))
        .await
        .expect("resolve ok");

    let kinds: Vec<&FetchKind> = resolution.fetch_plan.items.iter().map(|i| &i.kind).collect();

    assert!(
        kinds.contains(&&FetchKind::Manifest(manifest_hash)),
        "missing manifest must be in plan: {kinds:?}"
    );
    assert!(
        kinds.contains(&&FetchKind::Layer(l_missing)),
        "missing layer must be in plan: {kinds:?}"
    );
    assert!(
        !kinds.contains(&&FetchKind::Layer(l_present)),
        "present layer must NOT be in plan: {kinds:?}"
    );
}

/// And: when the store already has everything, the plan is empty.
#[tokio::test]
async fn fetch_plan_empty_when_store_has_everything() {
    let mut src = InMemorySource::new();
    let layer = synth_diff(0x55);
    let m = make_manifest_with_layers("acme", "thing", "1.0.0", vec![], vec![layer.clone()]);
    let manifest_hash = src.add(m);

    let mut store = StubStore::new();
    store.diffs.insert(layer);
    store.manifests.insert(manifest_hash);

    let root = RootRef {
        package: pkgref("acme", "thing"),
        version: vrange("^1"),
    };

    let resolution = resolve(&[root], &src, None, Some(&store))
        .await
        .expect("resolve ok");
    assert!(
        resolution.fetch_plan.items.is_empty(),
        "expected empty plan, got {:?}",
        resolution.fetch_plan.items
    );
}
