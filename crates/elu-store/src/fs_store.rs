use std::fs::{self, File};
use std::io::{self, Read, Write as _};

use bytes::Bytes;
use camino::{Utf8Path, Utf8PathBuf};
use fs2::FileExt;

use crate::atomic::{self, FsyncMode};
use crate::error::StoreError;
use crate::hash::{BlobId, DiffId, Hash, ManifestHash};
use crate::hasher::Hasher;
use crate::magic::{self, BlobEncoding};
use crate::store::{
    FsckError, FsckRepairReport, GcPlan, GcStats, ManifestReader, PutBlob, RefEntry, RefFilter,
    Store,
};

pub struct FsStore {
    root: Utf8PathBuf,
    fsync: FsyncMode,
}

impl FsStore {
    /// Open an existing store. Returns error if root doesn't exist.
    pub fn open(root: impl Into<Utf8PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        if !root.as_std_path().exists() {
            return Err(StoreError::RootMissing(root));
        }
        Ok(Self {
            root,
            fsync: FsyncMode::Always,
        })
    }

    /// Initialize a new store, creating directories as needed.
    pub fn init(root: impl Into<Utf8PathBuf>) -> Result<Self, StoreError> {
        Self::init_with_fsync(root, FsyncMode::Always)
    }

    /// Initialize with explicit fsync mode (use Never for tests).
    pub fn init_with_fsync(
        root: impl Into<Utf8PathBuf>,
        fsync: FsyncMode,
    ) -> Result<Self, StoreError> {
        let root = root.into();
        for dir in &["objects", "diffs", "manifests", "refs", "tmp", "locks"] {
            fs::create_dir_all(root.join(dir))?;
        }
        Ok(Self { root, fsync })
    }

    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    fn tmp_dir(&self) -> Utf8PathBuf {
        self.root.join("tmp")
    }

    fn blob_path(&self, id: &BlobId) -> Utf8PathBuf {
        let h = &id.0;
        self.root
            .join("objects")
            .join(h.algo().as_str())
            .join(h.prefix())
            .join(h.rest())
    }

    fn diff_path(&self, id: &DiffId) -> Utf8PathBuf {
        let h = &id.0;
        self.root
            .join("diffs")
            .join(h.algo().as_str())
            .join(h.prefix())
            .join(h.rest())
    }

    fn manifest_index_path(&self, id: &ManifestHash) -> Utf8PathBuf {
        self.root.join("manifests").join(id.0.to_string())
    }

    fn ref_path(&self, ns: &str, name: &str, version: &str) -> Utf8PathBuf {
        self.root.join("refs").join(ns).join(name).join(version)
    }

    fn gc_lock_path(&self) -> Utf8PathBuf {
        self.root.join("locks").join("gc.lock")
    }

    /// Mark + scan: enumerate every object/diff/tmp that GC would remove,
    /// without touching the store. Caller must hold an appropriate lock
    /// (shared for plan_gc, exclusive for gc) for the result to be consistent.
    fn compute_gc_plan(&self, reader: &dyn ManifestReader) -> Result<GcPlan, StoreError> {
        use std::collections::HashSet;

        let mut live_blobs: HashSet<String> = HashSet::new();
        let mut live_diffs: HashSet<String> = HashSet::new();

        let all_refs = self.list_refs(RefFilter::default())?;
        let mut manifest_queue: Vec<ManifestHash> =
            all_refs.iter().map(|r| r.hash.clone()).collect();
        let mut visited_manifests: HashSet<String> = HashSet::new();

        while let Some(mh) = manifest_queue.pop() {
            if !visited_manifests.insert(mh.to_string()) {
                continue;
            }
            live_blobs.insert(mh.0.to_string());

            if let Some(manifest_bytes) = self.get_manifest(&mh)? {
                if let Ok(diff_ids) = reader.layer_diff_ids(&manifest_bytes) {
                    for diff_id in &diff_ids {
                        live_diffs.insert(diff_id.to_string());
                        if let Ok(Some(blob_id)) = self.resolve_diff(diff_id) {
                            live_blobs.insert(blob_id.0.to_string());
                        }
                    }
                }
                if let Ok(deps) = reader.dependency_hashes(&manifest_bytes) {
                    for dep in deps {
                        manifest_queue.push(dep);
                    }
                }
            }
        }

        let mut plan = GcPlan::default();

        let objects_dir = self.root.join("objects");
        for blob_id_str in walk_hash_files(&objects_dir)? {
            if !live_blobs.contains(&blob_id_str) {
                let hash: Hash = blob_id_str.parse()?;
                let bid = BlobId(hash);
                let path = self.blob_path(&bid);
                if let Ok(meta) = fs::metadata(path.as_std_path()) {
                    plan.bytes_to_free += meta.len();
                }
                plan.objects_to_remove.push(bid);
            }
        }

        let diffs_dir = self.root.join("diffs");
        for diff_id_str in walk_hash_files(&diffs_dir)? {
            if !live_diffs.contains(&diff_id_str) {
                let hash: Hash = diff_id_str.parse()?;
                plan.diffs_to_remove.push(DiffId(hash));
            }
        }

        let tmp_dir = self.tmp_dir();
        if let Ok(entries) = fs::read_dir(tmp_dir.as_std_path()) {
            let cutoff = std::time::SystemTime::now()
                - std::time::Duration::from_secs(24 * 60 * 60);
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata()
                    && let Ok(modified) = meta.modified()
                    && modified < cutoff
                    && let Some(p) = camino::Utf8Path::from_path(&entry.path())
                {
                    plan.tmp_to_remove.push(p.to_path_buf());
                }
            }
        }

        Ok(plan)
    }

    /// Execute a previously computed plan. Caller must hold the exclusive
    /// gc lock; called only from `gc`.
    fn apply_gc_plan(&self, plan: &GcPlan) -> GcStats {
        let mut stats = GcStats {
            bytes_freed: plan.bytes_to_free,
            ..GcStats::default()
        };
        for bid in &plan.objects_to_remove {
            let path = self.blob_path(bid);
            let _ = fs::remove_file(path.as_std_path());
            stats.objects_removed += 1;
        }
        for did in &plan.diffs_to_remove {
            let path = self.diff_path(did);
            let _ = fs::remove_file(path.as_std_path());
            stats.diffs_removed += 1;
        }
        for path in &plan.tmp_to_remove {
            let _ = fs::remove_file(path.as_std_path());
            stats.tmp_removed += 1;
        }
        stats
    }
}

impl Store for FsStore {
    fn get(&self, id: &BlobId) -> Result<Option<Bytes>, StoreError> {
        let path = self.blob_path(id);
        match fs::read(path.as_std_path()) {
            Ok(data) => Ok(Some(Bytes::from(data))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    fn open(&self, id: &BlobId) -> Result<Option<File>, StoreError> {
        let path = self.blob_path(id);
        match File::open(path.as_std_path()) {
            Ok(f) => Ok(Some(f)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    fn has(&self, id: &BlobId) -> Result<bool, StoreError> {
        Ok(self.blob_path(id).as_std_path().exists())
    }

    fn size(&self, id: &BlobId) -> Result<Option<u64>, StoreError> {
        let path = self.blob_path(id);
        match fs::metadata(path.as_std_path()) {
            Ok(m) => Ok(Some(m.len())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    fn put_manifest(&self, bytes: &[u8]) -> Result<ManifestHash, StoreError> {
        // Validate: manifests must be valid UTF-8
        std::str::from_utf8(bytes).map_err(|_| StoreError::ManifestNotUtf8)?;

        let mut hasher = Hasher::new();
        hasher.update(bytes);
        let hash = hasher.finalize();
        let blob_id = BlobId(hash.clone());
        let manifest_hash = ManifestHash(hash);

        let dst = self.blob_path(&blob_id);
        if !dst.as_std_path().exists() {
            fs::create_dir_all(dst.parent().unwrap().as_std_path())?;
            let tmp = tempfile::NamedTempFile::new_in(self.tmp_dir().as_std_path())?;
            fs::write(tmp.path(), bytes)?;
            if self.fsync == FsyncMode::Always {
                let f = File::open(tmp.path())?;
                f.sync_data()?;
            }
            // rename into place; if it already exists (race), that's fine
            match tmp.persist(dst.as_std_path()) {
                Ok(_) => {
                    if self.fsync == FsyncMode::Always {
                        atomic::sync_parent_dir(&dst)?;
                    }
                }
                Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(StoreError::Rename {
                        to: dst,
                        err: e.error,
                    })
                }
            }
        }

        // Update manifests/ index
        let idx = self.manifest_index_path(&manifest_hash);
        if !idx.as_std_path().exists() {
            let _ = atomic::atomic_write(&idx, b"", self.fsync);
        }

        Ok(manifest_hash)
    }

    fn get_manifest(&self, id: &ManifestHash) -> Result<Option<Bytes>, StoreError> {
        let blob_id = BlobId(id.0.clone());
        self.get(&blob_id)
    }

    fn put_blob(&self, reader: &mut dyn Read) -> Result<PutBlob, StoreError> {
        let mut tmp = tempfile::NamedTempFile::new_in(self.tmp_dir().as_std_path())?;
        let mut blob_hasher = Hasher::new();
        let mut diff_hasher = Hasher::new();
        let mut stored_bytes: u64 = 0;
        let mut diff_bytes: u64 = 0;

        // Buffer first 512 bytes to detect encoding via magic bytes
        let mut peek_buf = vec![0u8; 512];
        let mut peeked = 0;
        while peeked < peek_buf.len() {
            match reader.read(&mut peek_buf[peeked..]) {
                Ok(0) => break,
                Ok(n) => peeked += n,
                Err(e) => return Err(StoreError::Io(e)),
            }
        }
        let peek = &peek_buf[..peeked];

        let encoding = magic::sniff_encoding(peek).ok_or(StoreError::UnknownEncoding)?;

        // Write peeked bytes to tmp and blob hasher
        tmp.write_all(peek)?;
        blob_hasher.update(peek);
        stored_bytes += peeked as u64;

        // Read the rest into tmp, hashing for blob_id
        let mut buf = [0u8; 64 * 1024];
        let mut rest_bytes = Vec::new();
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            tmp.write_all(&buf[..n])?;
            blob_hasher.update(&buf[..n]);
            rest_bytes.extend_from_slice(&buf[..n]);
            stored_bytes += n as u64;
        }

        // Build the full raw bytes for decompression
        let mut all_bytes = peek.to_vec();
        all_bytes.extend_from_slice(&rest_bytes);

        // Decompress and hash for diff_id
        match encoding {
            BlobEncoding::PlainTar => {
                // Identity: diff_id == blob_id
                diff_hasher.update(&all_bytes);
                diff_bytes = all_bytes.len() as u64;
            }
            BlobEncoding::Gzip => {
                let mut decoder = flate2::read::GzDecoder::new(&all_bytes[..]);
                let mut decomp_buf = [0u8; 64 * 1024];
                loop {
                    let n = decoder.read(&mut decomp_buf)?;
                    if n == 0 {
                        break;
                    }
                    diff_hasher.update(&decomp_buf[..n]);
                    diff_bytes += n as u64;
                }
            }
            BlobEncoding::Zstd => {
                let mut decoder = zstd::stream::read::Decoder::new(&all_bytes[..])?;
                let mut decomp_buf = [0u8; 64 * 1024];
                loop {
                    let n = decoder.read(&mut decomp_buf)?;
                    if n == 0 {
                        break;
                    }
                    diff_hasher.update(&decomp_buf[..n]);
                    diff_bytes += n as u64;
                }
            }
        }

        if self.fsync == FsyncMode::Always {
            tmp.as_file().sync_data()?;
        }

        let blob_id = BlobId(blob_hasher.finalize());
        let diff_id = DiffId(diff_hasher.finalize());

        // Rename blob into objects/
        let dst = self.blob_path(&blob_id);
        if !dst.as_std_path().exists() {
            fs::create_dir_all(dst.parent().unwrap().as_std_path())?;
            match tmp.persist(dst.as_std_path()) {
                Ok(_) => {
                    if self.fsync == FsyncMode::Always {
                        atomic::sync_parent_dir(&dst)?;
                    }
                }
                Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(StoreError::Rename {
                        to: dst,
                        err: e.error,
                    });
                }
            }
        }

        // Write diffs/ index (first-writer-wins)
        let diff_path = self.diff_path(&diff_id);
        if !diff_path.as_std_path().exists() {
            fs::create_dir_all(diff_path.parent().unwrap().as_std_path())?;
            // Use create_new to avoid overwriting
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(diff_path.as_std_path())
            {
                Ok(mut f) => {
                    use std::io::Write;
                    f.write_all(blob_id.0.to_string().as_bytes())?;
                    if self.fsync == FsyncMode::Always {
                        f.sync_data()?;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(StoreError::Io(e)),
            }
        }

        Ok(PutBlob {
            diff_id,
            blob_id,
            stored_bytes,
            diff_bytes,
        })
    }

    fn resolve_diff(&self, id: &DiffId) -> Result<Option<BlobId>, StoreError> {
        let path = self.diff_path(id);
        match fs::read_to_string(path.as_std_path()) {
            Ok(content) => {
                let hash: Hash = content.trim().parse()?;
                Ok(Some(BlobId(hash)))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    fn has_diff(&self, id: &DiffId) -> Result<bool, StoreError> {
        Ok(self.diff_path(id).as_std_path().exists())
    }

    fn put_ref(
        &self,
        ns: &str,
        name: &str,
        version: &str,
        hash: &ManifestHash,
    ) -> Result<(), StoreError> {
        validate_ref_component(ns)?;
        validate_ref_component(name)?;
        validate_ref_component(version)?;

        let path = self.ref_path(ns, name, version);

        // Check for conflict
        if let Ok(existing) = fs::read_to_string(path.as_std_path()) {
            let existing_trimmed = existing.trim();
            if existing_trimmed == hash.to_string() {
                return Ok(()); // idempotent
            }
            return Err(StoreError::RefConflict {
                path,
                existing: existing_trimmed.to_string(),
                incoming: hash.clone(),
            });
        }

        fs::create_dir_all(path.parent().unwrap().as_std_path())?;

        // Use shared GC lock for ref writes
        let lock_path = self.gc_lock_path();
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path.as_std_path())
            .map_err(StoreError::Lock)?;
        lock_file.lock_shared().map_err(StoreError::Lock)?;

        atomic::atomic_write(&path, hash.to_string().as_bytes(), self.fsync)?;

        lock_file.unlock().map_err(StoreError::Lock)?;
        Ok(())
    }

    fn get_ref(
        &self,
        ns: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<ManifestHash>, StoreError> {
        let path = self.ref_path(ns, name, version);
        match fs::read_to_string(path.as_std_path()) {
            Ok(content) => {
                let hash: Hash = content.trim().parse()?;
                Ok(Some(ManifestHash(hash)))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    fn remove_ref(&self, ns: &str, name: &str, version: &str) -> Result<(), StoreError> {
        validate_ref_component(ns)?;
        validate_ref_component(name)?;
        validate_ref_component(version)?;

        let path = self.ref_path(ns, name, version);

        let lock_path = self.gc_lock_path();
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path.as_std_path())
            .map_err(StoreError::Lock)?;
        lock_file.lock_shared().map_err(StoreError::Lock)?;

        let result = match fs::remove_file(path.as_std_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(StoreError::RefNotFound {
                namespace: ns.to_string(),
                name: name.to_string(),
                version: version.to_string(),
            }),
            Err(e) => Err(StoreError::Io(e)),
        };

        lock_file.unlock().map_err(StoreError::Lock)?;
        result
    }

    fn list_refs(&self, filter: RefFilter) -> Result<Vec<RefEntry>, StoreError> {
        let refs_dir = self.root.join("refs");
        let mut entries = Vec::new();

        let namespaces = match &filter.namespace {
            Some(ns) => vec![ns.clone()],
            None => list_subdirs(&refs_dir)?,
        };

        for ns in &namespaces {
            let ns_dir = refs_dir.join(ns);
            let names = match &filter.name {
                Some(n) => vec![n.clone()],
                None => list_subdirs(&ns_dir)?,
            };

            for name in &names {
                let name_dir = ns_dir.join(name);
                let versions = list_files(&name_dir)?;
                for version in &versions {
                    let path = name_dir.join(version);
                    if let Ok(content) = fs::read_to_string(path.as_std_path())
                        && let Ok(hash) = content.trim().parse::<Hash>()
                    {
                        entries.push(RefEntry {
                            namespace: ns.clone(),
                            name: name.clone(),
                            version: version.clone(),
                            hash: ManifestHash(hash),
                        });
                    }
                }
            }
        }

        Ok(entries)
    }

    fn gc(&self, reader: &dyn ManifestReader) -> Result<GcStats, StoreError> {
        let lock_path = self.gc_lock_path();
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path.as_std_path())
            .map_err(StoreError::Lock)?;
        lock_file
            .try_lock_exclusive()
            .map_err(|_| StoreError::GcBusy)?;

        let plan = self.compute_gc_plan(reader)?;
        let stats = self.apply_gc_plan(&plan);

        lock_file.unlock().map_err(StoreError::Lock)?;
        Ok(stats)
    }

    fn plan_gc(&self, reader: &dyn ManifestReader) -> Result<GcPlan, StoreError> {
        let lock_path = self.gc_lock_path();
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path.as_std_path())
            .map_err(StoreError::Lock)?;
        lock_file.lock_shared().map_err(StoreError::Lock)?;

        let plan = self.compute_gc_plan(reader);

        lock_file.unlock().map_err(StoreError::Lock)?;
        plan
    }

    fn fsck(&self) -> Result<Vec<FsckError>, StoreError> {
        let mut errors = Vec::new();

        // 1. Re-hash every object
        let objects_dir = self.root.join("objects");
        for blob_id_str in walk_hash_files(&objects_dir)? {
            let hash: Hash = blob_id_str.parse()?;
            let bid = BlobId(hash);
            let path = self.blob_path(&bid);
            if let Ok(data) = fs::read(path.as_std_path()) {
                let mut hasher = Hasher::new();
                hasher.update(&data);
                let actual = BlobId(hasher.finalize());
                if actual != bid {
                    errors.push(FsckError::HashMismatch {
                        path: path.to_string(),
                        expected: bid.to_string(),
                        actual: actual.to_string(),
                    });
                }
            }
        }

        // 2. Check diffs/ entries point to existing blobs
        let diffs_dir = self.root.join("diffs");
        for diff_id_str in walk_hash_files(&diffs_dir)? {
            let hash: Hash = diff_id_str.parse()?;
            let did = DiffId(hash);
            let path = self.diff_path(&did);
            if let Ok(content) = fs::read_to_string(path.as_std_path())
                && let Ok(blob_hash) = content.trim().parse::<Hash>()
            {
                let bid = BlobId(blob_hash);
                if !self.blob_path(&bid).as_std_path().exists() {
                    errors.push(FsckError::OrphanedDiff {
                        diff_id: did.to_string(),
                        blob_id: bid.to_string(),
                    });
                }
            }
        }

        // 3. Check refs point to existing manifests
        let refs = self.list_refs(RefFilter::default())?;
        for r in &refs {
            let blob_id = BlobId(r.hash.0.clone());
            if !self.blob_path(&blob_id).as_std_path().exists() {
                errors.push(FsckError::BrokenRef {
                    ref_path: format!("{}/{}/{}", r.namespace, r.name, r.version),
                    target: r.hash.to_string(),
                });
            }
        }

        Ok(errors)
    }

    fn fsck_repair(&self) -> Result<FsckRepairReport, StoreError> {
        let _ = self;
        unimplemented!("fsck_repair")
    }
}

fn validate_ref_component(s: &str) -> Result<(), StoreError> {
    if s.is_empty()
        || s.contains('/')
        || s.contains('\\')
        || s.contains("..")
        || s.chars().any(|c| c.is_control())
    {
        return Err(StoreError::InvalidRefComponent(s.to_string()));
    }
    Ok(())
}

fn list_subdirs(path: &Utf8Path) -> Result<Vec<String>, StoreError> {
    let mut names = Vec::new();
    match fs::read_dir(path.as_std_path()) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir()
                    && let Some(name) = entry.file_name().to_str()
                {
                    names.push(name.to_string());
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(StoreError::Io(e)),
    }
    Ok(names)
}

fn list_files(path: &Utf8Path) -> Result<Vec<String>, StoreError> {
    let mut names = Vec::new();
    match fs::read_dir(path.as_std_path()) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_file()
                    && let Some(name) = entry.file_name().to_str()
                {
                    names.push(name.to_string());
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(StoreError::Io(e)),
    }
    Ok(names)
}

/// Walk `<dir>/<algo>/<prefix>/<rest>` and return `"<algo>:<prefix><rest>"` strings.
fn walk_hash_files(base: &Utf8Path) -> Result<Vec<String>, StoreError> {
    let mut results = Vec::new();
    let algos = list_subdirs(base)?;
    for algo in &algos {
        let algo_dir = base.join(algo);
        let prefixes = list_subdirs(&algo_dir)?;
        for prefix in &prefixes {
            let prefix_dir = algo_dir.join(prefix);
            let rests = list_files(&prefix_dir)?;
            for rest in &rests {
                results.push(format!("{algo}:{prefix}{rest}"));
            }
        }
    }
    Ok(results)
}
