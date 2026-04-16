# elu-store: Content-Addressed Store

Implementation design for the CAS described in
[../prd/store.md](../prd/store.md). This crate owns every byte elu
writes under the store root, owns the shared hash types (`Hash`,
`DiffId`, `BlobId`), and is the only crate that knows the on-disk
layout. Higher rings go through the `Store` trait.

---

## Scope

- Hash types and the sha256 implementation wrapper.
- `Store` trait: the sole public interface higher rings consume.
- `FsStore`: the v1 implementation of `Store` over a local directory.
- Atomic write discipline (`tmp/` + rename).
- `diffs/` index.
- Refs (`refs/<ns>/<name>/<version>`).
- GC: mark-and-sweep with an exclusive file lock.
- `fsck`: re-hashes every object.

Out of scope for this crate: manifest parsing (lives in
`elu-manifest`), tar decompression (lives in `elu-layers`), HTTP
fetching (lives in `elu-registry`). The store is pure
byte-in/byte-out.

---

## Hash types

```rust
// crates/elu-store/src/hash.rs

/// Algorithm tag in the canonical `<algo>:<hex>` prefix.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum HashAlgo {
    Sha256,
}

/// A content hash with its algorithm tag. Stored as 32 bytes + tag
/// (48 bytes total for sha256). Display/FromStr use `sha256:<hex>`.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Hash {
    algo: HashAlgo,
    bytes: [u8; 32],
}

impl Hash {
    pub fn algo(&self) -> HashAlgo { self.algo }
    pub fn bytes(&self) -> &[u8; 32] { &self.bytes }
    pub fn prefix(&self) -> &str { /* "ab" */ }
    pub fn rest(&self) -> String { /* "cdef..." */ }
}

impl fmt::Display for Hash { /* "sha256:<hex>" */ }
impl FromStr  for Hash    { type Err = HashParseError; }

/// Newtypes over Hash. Separate types make the diff_id/blob_id
/// distinction a compile-time property, not a convention.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DiffId(pub Hash);

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct BlobId(pub Hash);

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ManifestHash(pub Hash);  // equal to a BlobId bytewise;
                                    //   the type carries intent.
```

Rationale for the newtypes: the PRD is very precise that diff_id and
blob_id have different invariants and different storage paths.
Passing a `BlobId` to a function that wants a `DiffId` is a bug, and
the type system should catch it for free.

The algorithm tag is part of the value, not a compile-time parameter,
so we can migrate to a new algorithm without rewriting every
signature.

### Hashing

```rust
// crates/elu-store/src/hasher.rs
use sha2::{Digest, Sha256};

pub struct Hasher {
    inner: Sha256,
}

impl Hasher {
    pub fn new() -> Self { Self { inner: Sha256::new() } }
    pub fn update(&mut self, chunk: &[u8]) { self.inner.update(chunk); }
    pub fn finalize(self) -> Hash {
        let out: [u8; 32] = self.inner.finalize().into();
        Hash { algo: HashAlgo::Sha256, bytes: out }
    }
}
```

Thin wrapper over `sha2::Sha256`. We don't export `sha2` through the
public API — callers use `Hasher`. This keeps algorithm migration a
one-file change.

Why sha256 and not blake3: OCI byte-compatibility. See
[overview.md](overview.md#boring-defaults).

---

## The `Store` trait

Higher rings code against this trait. The v1 implementation is
`FsStore` (next section); a future `HttpStore` or `S3Store` would
implement the same trait without changing any consumer.

```rust
// crates/elu-store/src/store.rs

pub trait Store {
    // --- Blob access (CAS keyed by blob_id) ---

    fn get(&self, id: &BlobId) -> Result<Option<Bytes>, StoreError>;
    fn open(&self, id: &BlobId) -> Result<Option<File>, StoreError>;
    fn has(&self, id: &BlobId) -> Result<bool, StoreError>;
    fn size(&self, id: &BlobId) -> Result<Option<u64>, StoreError>;

    // --- Layer blobs (compute both ids; index diff_id → blob_id) ---
    //
    // `put_blob` takes a reader of raw bytes (possibly compressed).
    // It streams bytes to disk under tmp/, hashes the raw bytes into
    // blob_id, runs a parallel decompression stream (sniffing magic
    // bytes) to produce the uncompressed bytes, hashes those into
    // diff_id, and on success renames into objects/ and writes the
    // diffs/ index entry.
    fn put_blob(&self, bytes: &mut dyn Read) -> Result<PutBlob, StoreError>;
    fn resolve_diff(&self, id: &DiffId) -> Result<Option<BlobId>, StoreError>;
    fn has_diff(&self, id: &DiffId) -> Result<bool, StoreError>;

    // --- Manifests (blob_id == diff_id; always uncompressed) ---

    fn put_manifest(&self, bytes: &[u8]) -> Result<ManifestHash, StoreError>;
    fn get_manifest(&self, id: &ManifestHash) -> Result<Option<Bytes>, StoreError>;

    // --- Refs ---

    fn put_ref(
        &self,
        ns: &str, name: &str, version: &str,
        hash: &ManifestHash,
    ) -> Result<(), StoreError>;
    fn get_ref(
        &self,
        ns: &str, name: &str, version: &str,
    ) -> Result<Option<ManifestHash>, StoreError>;
    fn list_refs(&self, filter: RefFilter) -> Result<Vec<RefEntry>, StoreError>;

    // --- Maintenance ---

    fn gc(&self) -> Result<GcStats, StoreError>;
    fn fsck(&self) -> Result<Vec<FsckError>, StoreError>;
}

#[derive(Debug)]
pub struct PutBlob {
    pub diff_id: DiffId,
    pub blob_id: BlobId,
    pub stored_bytes: u64,
    pub diff_bytes: u64,
}
```

`Bytes` is an owned byte buffer (we use `bytes::Bytes` for cheap
clones of read-only memory). `File` is `std::fs::File` so consumers
can stream without copying.

The trait is `&self`, not `&mut self`: a store supports concurrent
readers and writers, with atomicity provided by the filesystem
rename + the GC lock. Interior mutability is via the filesystem
itself, not via mutable fields.

Note: the trait is **sync**. The registry client handles async work
and then calls back into the store via `spawn_blocking`.

---

## `FsStore`: the filesystem implementation

```rust
// crates/elu-store/src/fs_store.rs
use camino::Utf8PathBuf;

pub struct FsStore {
    root: Utf8PathBuf,
}

impl FsStore {
    pub fn open(root: impl Into<Utf8PathBuf>) -> Result<Self, StoreError>;
    pub fn init(root: impl Into<Utf8PathBuf>) -> Result<Self, StoreError>;
    pub fn root(&self) -> &Utf8Path;
}

impl Store for FsStore { /* ... */ }
```

`open` requires the layout already exists; `init` creates it. The
CLI's `elu init-store` (if we add one) and the first `elu`
invocation both call `init` with an "ok if exists" flag.

### Layout

Exactly as specified in the PRD:

```
<root>/
  objects/sha256/<ab>/<rest>     # blob, filename = blob_id.rest()
  diffs/sha256/<ab>/<rest>       # one-line file containing the blob_id
  manifests/<blob_id>            # cache index; filenames only
  refs/<ns>/<name>/<version>     # one-line file containing the manifest hash
  tmp/<random>                   # staging for atomic writes
  locks/gc.lock                  # flock target for GC exclusivity
```

Resolution:

```rust
fn blob_path(&self, id: &BlobId) -> Utf8PathBuf {
    let h = &id.0;
    self.root
        .join("objects")
        .join(h.algo().as_str())    // "sha256"
        .join(h.prefix())           // first 2 hex chars
        .join(h.rest())             // remaining 62 chars
}
```

The two-hex fan-out matches the PRD and keeps any single directory
under ~4096 entries for stores that reach ~1M objects.

### Atomic writes

Every write follows the same shape:

1. Create `tmp/<rand>` via `tempfile::NamedTempFile::new_in(tmp_dir)`.
2. Stream bytes into the tmp file, updating hashers as we go.
3. `file.sync_data()` before rename — durability for writers that
   care about crash recovery. Controlled by an `FsyncMode` knob:
   `Always` (default), `Never` (for tests and ephemeral CI stores).
4. Compute the destination path from the finalized hash.
5. If the destination already exists, drop the tmp file (dedupe
   win). Otherwise `rename(tmp, dest)`.
6. `sync_data()` on the parent directory to persist the rename.

On Linux/macOS, `rename(2)` is atomic within a filesystem and the
tmp directory lives under the store root, so rename is always
intra-filesystem. On Windows, `std::fs::rename` over an existing
target fails by default — we use `fs::rename` for the common case
(target absent after the `exists` check) and accept that the tiny
race where two writers race to create the same blob ends with one
getting `AlreadyExists`, which we treat as success.

### `put_blob` (two-hash pass)

```rust
fn put_blob(&self, reader: &mut dyn Read) -> Result<PutBlob, StoreError> {
    let mut tmp = NamedTempFile::new_in(&self.tmp_dir())?;
    let mut blob_hasher = Hasher::new();
    let mut diff_hasher = Hasher::new();

    // Peek the first 8 bytes to sniff the compression magic.
    // We buffer these into the tmp file AND into the decompressor.
    let mut peek = [0u8; 8];
    let peeked = read_fully(reader, &mut peek)?;
    let magic = sniff_magic(&peek[..peeked]);

    let mut decomp = open_decompressor(magic, &peek[..peeked]);

    // Include the peeked bytes in both streams.
    tmp.write_all(&peek[..peeked])?;
    blob_hasher.update(&peek[..peeked]);
    decomp.write_all_compressed(&peek[..peeked])?;
    while let Some(plain) = decomp.drain_available()? {
        diff_hasher.update(&plain);
    }

    // Stream the rest.
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        tmp.write_all(&buf[..n])?;
        blob_hasher.update(&buf[..n]);
        decomp.write_all_compressed(&buf[..n])?;
        while let Some(plain) = decomp.drain_available()? {
            diff_hasher.update(&plain);
        }
    }
    for plain in decomp.finish()? {
        diff_hasher.update(&plain);
    }

    tmp.as_file_mut().sync_data()?;

    let blob_id = BlobId(blob_hasher.finalize());
    let diff_id = DiffId(diff_hasher.finalize());

    let dst = self.blob_path(&blob_id);
    if !dst.exists() {
        std::fs::create_dir_all(dst.parent().unwrap())?;
        tmp.persist(&dst)
            .map_err(|e| StoreError::Rename { to: dst.clone(), err: e.error })?;
        sync_parent_dir(&dst)?;
    }

    // Update diffs/ index (first-writer-wins per PRD).
    let diff_path = self.diff_path(&diff_id);
    if !diff_path.exists() {
        std::fs::create_dir_all(diff_path.parent().unwrap())?;
        atomic_write(&diff_path, blob_id.to_string().as_bytes())?;
    }

    Ok(PutBlob { diff_id, blob_id, stored_bytes: ..., diff_bytes: ... })
}
```

Magic-byte sniffing table:

| Bytes | Encoding | Decompressor |
|---|---|---|
| `28 b5 2f fd` | zstd | `zstd::stream::read::Decoder` |
| `1f 8b` | gzip | `flate2::read::GzDecoder` |
| `"ustar"` at offset 257 | plain tar | identity passthrough |

If none match, `put_blob` returns `StoreError::UnknownEncoding`.

### `diffs/` index — first-writer-wins

Per the PRD, if the `diffs/<diff_id>` file already exists, the new
blob is still written to `objects/` but is **not** indexed; GC will
collect it. We do not overwrite. Implementation: `OpenOptions` with
`create_new(true)`, and we swallow `ErrorKind::AlreadyExists` as a
success.

### Refs

```rust
fn put_ref(&self, ns: &str, name: &str, version: &str, hash: &ManifestHash)
    -> Result<(), StoreError>
{
    validate_ref_component(ns)?;
    validate_ref_component(name)?;
    validate_ref_component(version)?;
    let path = self.root
        .join("refs").join(ns).join(name).join(version);
    std::fs::create_dir_all(path.parent().unwrap())?;
    // Refs are append-only in practice. If the file already exists
    // and has a different hash, we refuse; the registry layer is
    // the one that rejects re-publishing, but we enforce the local
    // invariant too.
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing.trim() != hash.to_string() {
            return Err(StoreError::RefConflict { path, existing, incoming: hash.clone() });
        }
        return Ok(());
    }
    atomic_write(&path, hash.to_string().as_bytes())?;
    Ok(())
}
```

`validate_ref_component` rejects components containing `/`, `\`,
`..`, or control characters. Per the PRD, names and namespaces are
lowercase dash-separated; we enforce the character set at the store
boundary as defense in depth even though `elu-manifest` has already
validated manifests before they reach us.

### Locks

GC takes an exclusive advisory lock on `locks/gc.lock` via the `fs2`
crate (`flock(LOCK_EX)` on Unix, `LockFileEx` on Windows). Writers
take a **shared** lock on the same file for the duration of a
`put_ref` call — this is the "hold gc.lock during publish" rule from
the PRD, minimized to just the ref commit step rather than the full
blob write. `put_blob` does not take any lock because blob writes
are idempotent and hash-keyed; a blob being written concurrently
with GC is fine — GC only collects blobs not reachable from a ref,
and a blob not yet referenced is legitimately garbage until the ref
exists.

```rust
struct GcLock { file: File, mode: LockMode }
enum LockMode { Shared, Exclusive }

impl FsStore {
    fn lock_shared(&self)    -> Result<GcLock, StoreError> { /* ... */ }
    fn lock_exclusive(&self) -> Result<GcLock, StoreError> { /* ... */ }
}
```

`lock_shared` waits if GC is running. `lock_exclusive` waits if any
publisher is committing a ref.

### `fsck`

```rust
fn fsck(&self) -> Result<Vec<FsckError>, StoreError> {
    let mut errors = Vec::new();
    // 1. For every file under objects/, re-hash and compare to filename.
    // 2. For every file under diffs/, check the referenced blob_id
    //    exists in objects/, and that decompressing it reproduces the
    //    diff_id in the path.
    // 3. For every file under refs/, check the referenced manifest
    //    blob_id exists in objects/.
    Ok(errors)
}
```

Expensive — re-hashes every byte. Exposed via `elu fsck`. Runs under
a shared GC lock so it doesn't race a collection.

### `gc`

Direct translation of the PRD pseudocode. Two-phase:

1. **Mark.** Take `locks/gc.lock` exclusive. Walk `refs/`, parse each
   referenced manifest, collect the transitive closure of manifest
   blob_ids and layer diff_ids. Resolve each diff_id to its blob_id
   via the `diffs/` index and add that blob_id to the live set.
2. **Sweep.** Walk `objects/`, unlink any file whose filename is not
   in the live set. Walk `diffs/`, unlink any file whose diff_id is
   not in the live set. Remove `tmp/` entries older than 24h.

GC parses manifests to walk their layer lists. This is the only
place in `elu-store` that reads manifest content, and it goes
through a small trait that `elu-manifest` implements:

```rust
pub trait ManifestReader {
    fn layer_diff_ids(&self, bytes: &[u8]) -> Result<Vec<DiffId>, ManifestReadError>;
    fn dependency_hashes(&self, bytes: &[u8]) -> Result<Vec<ManifestHash>, ManifestReadError>;
}
```

`elu-manifest` implements this trait; `elu-store` takes an
`&dyn ManifestReader` on the `gc` call. This keeps the dependency
direction correct — store still doesn't depend on manifest — while
letting GC walk the graph.

---

## Concurrency model

- **Reads are lock-free.** Open, hash-verify on boundaries only (we
  trust the filesystem within a session). A reader that sees a file
  in `objects/` is guaranteed it's complete.
- **`put_blob` is lock-free.** Rename is atomic; concurrent writers
  producing the same blob converge.
- **`put_ref` takes a shared GC lock.** Cheap.
- **`gc` and `fsck` take the exclusive GC lock.** Cheap (locks a
  single file).

The `Store` trait is `Send + Sync`. `FsStore` is cheaply cloneable
(it's just a `Utf8PathBuf`), so crates that need per-thread stores
clone it rather than wrapping it in `Arc`.

---

## Errors

```rust
#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("store root not found at {0}")]
    RootMissing(Utf8PathBuf),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unknown blob encoding")]
    UnknownEncoding,

    #[error("ref conflict at {path}: existing={existing}, incoming={incoming}")]
    RefConflict { path: Utf8PathBuf, existing: String, incoming: ManifestHash },

    #[error("hash parse error: {0}")]
    HashParse(#[from] HashParseError),

    #[error("rename failed to {to}")]
    Rename { to: Utf8PathBuf, err: std::io::Error },

    #[error("gc locked: another process is running gc")]
    GcBusy,

    #[error("lock i/o: {0}")]
    Lock(std::io::Error),
}
```

Error codes (stable across versions; used by `--json` output):

| Code | Variant |
|---|---|
| `store.root_missing` | `RootMissing` |
| `store.io` | `Io` |
| `store.unknown_encoding` | `UnknownEncoding` |
| `store.ref_conflict` | `RefConflict` |
| `store.hash_parse` | `HashParse` |
| `store.rename` | `Rename` |
| `store.gc_busy` | `GcBusy` |
| `store.lock` | `Lock` |

---

## What this crate does **not** do

- **No HTTP.** Fetching a missing blob from a registry is
  `elu-registry`'s job. The store returns `not_found`; a caller
  decides whether to fetch and retry.
- **No manifest parsing** beyond the tiny `ManifestReader` trait
  used by GC.
- **No tar handling.** `elu-layers` owns that. The store hands out
  file handles; layers does the decompression and walks the tar.
- **No locking beyond `gc.lock`.** Per-blob, per-ref fine-grained
  locks are unnecessary given hash addressing.
- **No remote backends.** `S3Store`, `HttpStore`, etc. are future
  work. The `Store` trait exists so they can be added without
  touching consumers, but v1 ships only `FsStore`.

---

## Testing strategy

- **Unit tests (`src/`)**: hash types round-trip, magic-byte sniff
  table, path math, atomic-write primitives, validation predicates.
- **Integration tests (`tests/`)**: build a temp store via
  `FsStore::init`, exercise every `Store` trait method against it,
  simulate crashes by dropping tmp files mid-write, run GC and verify
  reachability, run fsck on a known-corrupt layout and verify the
  error list.
- **Property tests**: for `put_blob`, generate random byte streams
  in each supported encoding and assert `store.resolve_diff(diff_id)
  .and_then(store.get)` round-trips to the original uncompressed
  bytes.

The integration tests use `tempfile::TempDir` so tests are fully
hermetic and parallel-safe.
