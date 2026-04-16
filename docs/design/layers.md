# elu-layers: Tar, Compression, Whiteouts, Stacker

Implementation design for layer handling described in
[../prd/layers.md](../prd/layers.md). This crate turns stored blob
bytes back into a materialized file tree, and turns a file tree
into a stored blob. It does not own the CAS (that's `elu-store`)
and it does not own the manifest graph (that's `elu-resolver`).

---

## Scope

- Tar writing (for the build pipeline) and tar reading (for
  stacking).
- Compression: zstd, gzip, plain. Magic-byte sniffing.
- Whiteout and opaque-whiteout handling.
- The `Stacker`: apply an ordered list of `DiffId`s into a staging
  directory, with last-layer-wins semantics.
- Path canonicalization and safety checks.
- Integration with `elu-hooks`: after stacking, hand off the
  staging directory to the hook interpreter.

Out of scope: resolving manifest graphs to a diff_id list (that's
`elu-resolver::flatten`); the `elu build` pipeline (that's
`elu-cli`; this crate provides the tar-write primitive it calls);
output format materialization (that's `elu-outputs`).

---

## Compression

```rust
// crates/elu-layers/src/compression.rs

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Encoding {
    Plain,
    Gzip,
    Zstd,
}

impl Encoding {
    /// Sniff the encoding from the first bytes of a blob. Returns
    /// `None` if no known magic matches.
    pub fn sniff(peek: &[u8]) -> Option<Encoding>;
}

/// Wrap a reader in the appropriate decompressor.
pub fn decode(enc: Encoding, r: impl Read) -> Box<dyn Read>;

/// Wrap a writer in a compressor for layer production.
pub fn encode(enc: Encoding, w: impl Write) -> Box<dyn Write>;
```

Sniff table (identical to the one in `elu-store::put_blob`; we
factor it into this crate and both consumers use it):

| Bytes | Encoding |
|---|---|
| `28 b5 2f fd` | `Zstd` |
| `1f 8b` | `Gzip` |
| `"ustar"` at offset 257 | `Plain` |

Zstd uses the `zstd` crate (`zstd::stream::read::Decoder`,
`zstd::stream::write::Encoder`). Gzip uses `flate2::read::GzDecoder`
and `flate2::write::GzEncoder`. Plain is the identity.

The default encoding for `elu build` is **zstd, level 3**. Rationale:
zstd at level 3 compresses ~as well as gzip at level 6 while being
several times faster; level 3 is the Zstandard reference default and
what every other modern tool uses. Higher levels cost CPU for
diminishing size wins. The CLI exposes `--compression none|gzip|zstd`
and `--level N`; the defaults are zstd/3.

Why support gzip at all: interop. An importer consuming OCI layers
or an apt repository's `.tar.gz` archives needs to store what it
received without recompressing (the `diff_id` stays the same;
recompressing would produce a new `blob_id` that the store already
knows how to handle, but the round-trip would be pure waste).

---

## Tar

### Writing

```rust
// crates/elu-layers/src/tar/write.rs
use tar::Builder;

/// Append entries from the given source specification, producing
/// a deterministic tar stream.
pub struct LayerBuilder<W: Write> {
    inner: Builder<W>,
    opts: LayerBuildOpts,
}

#[derive(Clone, Debug)]
pub struct LayerBuildOpts {
    /// Use epoch-0 mtimes for every entry. Load-bearing for
    /// reproducible diff_ids.
    pub zero_mtime: bool,
    /// Force uid/gid to 0 for every entry.
    pub zero_ownership: bool,
    /// Byte-sort entries by path before writing. Load-bearing for
    /// reproducibility: two builds of the same tree on different
    /// filesystems must produce the same tar.
    pub sort_entries: bool,
}

impl Default for LayerBuildOpts {
    fn default() -> Self {
        Self { zero_mtime: true, zero_ownership: true, sort_entries: true }
    }
}

impl<W: Write> LayerBuilder<W> {
    pub fn new(w: W, opts: LayerBuildOpts) -> Self;
    pub fn append_file(&mut self, path: &Utf8Path, mode: u32, bytes: &[u8])
        -> Result<(), LayerError>;
    pub fn append_dir(&mut self, path: &Utf8Path, mode: u32)
        -> Result<(), LayerError>;
    pub fn append_symlink(&mut self, path: &Utf8Path, target: &Utf8Path, mode: u32)
        -> Result<(), LayerError>;
    /// Finalize: pads the tar, flushes the inner writer, returns it.
    pub fn finish(self) -> Result<W, LayerError>;
}
```

Reproducibility is the first concern. Two invocations of `elu build`
on the same source files on different machines must produce the
same `diff_id`. That requires:

- **Zero mtimes.** The tar format encodes mtimes per entry; tar
  readers use them for extraction. Real filesystem mtimes embed
  build-time noise. We use `0` unconditionally.
- **Zero uid/gid.** For the same reason. The CLI can override to
  preserve ownership (`--preserve-ownership`) for importers that
  genuinely need it, but the default is zero.
- **Sorted entries.** `std::fs::read_dir` does not guarantee order.
  We collect, sort by byte-wise UTF-8 path comparison, and append
  in order.
- **No PAX headers for user/group names.** The `tar` crate emits
  them by default when a numeric uid has no name lookup; we disable
  name lookups and emit numeric-only headers. This keeps byte
  output independent of the build host's `/etc/passwd`.
- **No sparse-file encoding.** Files are written as plain tar
  regular entries even if they contain long runs of zeroes. Sparse
  encoding is tar-format-dependent and varies by producer.

### Reading

```rust
// crates/elu-layers/src/tar/read.rs
use tar::Archive;

pub struct LayerReader<R: Read> {
    archive: Archive<R>,
}

impl<R: Read> LayerReader<R> {
    pub fn new(r: R) -> Self;
    pub fn entries(&mut self) -> impl Iterator<Item = Result<Entry, LayerError>>;
}

#[derive(Debug)]
pub enum Entry {
    File { path: Utf8PathBuf, mode: u32, data: Box<dyn Read> },
    Dir  { path: Utf8PathBuf, mode: u32 },
    Symlink { path: Utf8PathBuf, target: String, mode: u32 },
    Whiteout { parent: Utf8PathBuf, target: String },
    OpaqueWhiteout { parent: Utf8PathBuf },
    // Hardlinks are preserved if present. We model them as
    // `Hardlink { path, link_target }` and resolve them in the
    // stacker by copying the referenced entry's content.
    Hardlink { path: Utf8PathBuf, target: Utf8PathBuf, mode: u32 },
}
```

The reader canonicalizes every path through `Utf8Path` and rejects
entries whose path:

- contains `..` components that would escape the layer root,
- is absolute,
- is not valid UTF-8 (tar headers are 8-bit, but `ustar` header
  names are conventionally ASCII/UTF-8; a non-UTF-8 name is
  rejected),
- contains a NUL byte,
- on Windows, contains a reserved character (`<>:"|?*`). Windows
  extraction replaces these with `_` and logs a warning; strict
  mode rejects them.

Whiteout detection:

- A file entry with basename `.wh..wh..opq` becomes
  `Entry::OpaqueWhiteout { parent }`.
- A file entry with basename `.wh.<something>` becomes
  `Entry::Whiteout { parent, target: <something> }`.
- All other files become `Entry::File`.

The reader consumes the tar stream and yields borrowed-ish entries:
for `File`, the `data` box is a reader pinned to the archive, so the
caller must consume `data` before pulling the next entry (this
matches `tar::Archive`'s API).

---

## Stacker

```rust
// crates/elu-layers/src/stack.rs

pub struct Stacker<'s, S: elu_store::Store> {
    store: &'s S,
    target: Utf8PathBuf,
    opts: StackOpts,
}

#[derive(Clone, Debug, Default)]
pub struct StackOpts {
    /// Fail if the target directory exists and is non-empty.
    /// Default: true.
    pub require_empty_target: bool,
    /// Preserve symlinks from layers as symlinks in the output.
    /// Disabling materializes symlinks as copies; used by qcow2.
    pub preserve_symlinks: bool,
    /// Apply umask to mode bits. Default: no umask.
    pub umask: Option<u32>,
}

impl<'s, S: elu_store::Store> Stacker<'s, S> {
    pub fn new(store: &'s S, target: impl Into<Utf8PathBuf>) -> Self;
    pub fn with_opts(mut self, opts: StackOpts) -> Self;

    /// Apply the given ordered diff_ids into the target directory.
    /// Later layers win on path collision. Whiteouts delete entries
    /// from the merged state so far.
    pub fn apply(&self, diff_ids: &[DiffId]) -> Result<StackStats, LayerError>;
}

#[derive(Debug, Default)]
pub struct StackStats {
    pub layers_applied: usize,
    pub bytes_written: u64,
    pub files_written: u64,
    pub dirs_written: u64,
    pub entries_removed: u64,  // from whiteouts
}
```

### Algorithm

```
apply(diff_ids):
    if require_empty_target and target not empty: error
    mkdir -p target
    for each diff_id in diff_ids (in order):
        blob_id = store.resolve_diff(diff_id)? or error MissingLayer
        file = store.open(&blob_id)?
        peek 8 bytes to sniff encoding
        reader = decode(encoding, file)
        layer = LayerReader::new(reader)
        for entry in layer.entries():
            apply_entry(entry, target)
    return stats
```

`apply_entry` handles each variant:

- **`File`**: compute `dest = target.join(path)`. If the destination
  exists as a directory, that's an error (a file replacing a dir is
  an OCI-level invariant; we mirror it). If it exists as a file or
  symlink, remove and rewrite. `write(dest, data)` streams the
  entry's data into place, then `chmod(dest, mode & !umask)`. The
  parent directory is created with default mode (`0755`) if it
  doesn't exist — tar streams do include directory entries before
  their children when well-formed, but we defensively create on
  demand.

- **`Dir`**: if absent, create with the entry's mode. If present as
  a directory, update the mode. If present as a file or symlink,
  error.

- **`Symlink`**: `symlink(target, dest)`. Remove any pre-existing
  entry at `dest`. The target is written verbatim; it's resolved
  at runtime when the output is used, not at stack time.

- **`Whiteout`**: compute `victim = target.join(parent).join(name)`.
  If it exists, remove it recursively. The whiteout entry itself
  is not materialized.

- **`OpaqueWhiteout`**: recursively remove every child of
  `target.join(parent)` without removing the parent directory
  itself. Subsequent entries in this same layer that land in the
  same directory are applied on top of the emptied state.

- **`Hardlink`**: resolve the target (must already exist in the
  merged tree; tar requires hardlink targets to precede). Copy the
  content from the target entry. We do not preserve hardlinks as
  hardlinks in the output — the PRD doesn't require it and
  preserving them across merges creates consistency bugs.

### Safety guarantees enforced by the stacker

- **No escape via `..`.** Every path is joined via a helper that
  verifies the final canonicalized path still lives under `target`.
  Absolute paths are rejected at the reader.
- **No symlink-following during writes.** We never write through an
  existing symlink. If the destination is a symlink, we unlink the
  symlink and create a fresh entry at the path. This closes the
  classic "symlink points at /etc/passwd" attack where an earlier
  layer creates a symlink to a host path and a later layer writes
  through it.
- **Mode bits are masked to the Unix mode set.** setuid/setgid are
  preserved (they're part of the layer's declared content), but the
  stacker optionally clears them via `StackOpts::clear_suid` for
  outputs that ship to untrusted consumers.

---

## Integration with hooks

After `Stacker::apply` finishes, the caller (typically an output
format or `elu install`) runs the post-unpack hook. The stacker
does not call the hook interpreter directly — it returns the
staging path and the caller invokes `elu_hooks::run(&manifest,
&staging)`.

Separation of concerns: the stacker knows about tars and
directories; the hook interpreter knows about manifest ops and
policy. A consumer that wants to stack without running hooks (e.g.
for inspecting a package's raw contents) just doesn't call the
hook step.

```rust
// typical caller flow
let stats = Stacker::new(&store, &staging).apply(&diff_ids)?;
elu_hooks::run(&manifest, &staging, &hook_policy)?;
output.finalize(&staging, &dest)?;
```

---

## The `LayerError` type

```rust
#[derive(thiserror::Error, Debug)]
pub enum LayerError {
    #[error("store: {0}")]
    Store(#[from] elu_store::StoreError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("tar: {0}")]
    Tar(std::io::Error),   // tar crate reports io::Error

    #[error("missing layer: diff_id {0} not found in store")]
    MissingLayer(DiffId),

    #[error("unknown encoding in blob {0}")]
    UnknownEncoding(BlobId),

    #[error("path escapes staging root: {0}")]
    PathEscape(String),

    #[error("file-replaces-directory at {0}")]
    FileReplacesDir(Utf8PathBuf),

    #[error("dir-replaces-file at {0}")]
    DirReplacesFile(Utf8PathBuf),

    #[error("invalid path {0}: {reason}")]
    InvalidPath { path: String, reason: &'static str },
}
```

Error codes: `layers.store`, `layers.io`, `layers.tar`,
`layers.missing_layer`, `layers.unknown_encoding`,
`layers.path_escape`, `layers.file_replaces_dir`,
`layers.dir_replaces_file`, `layers.invalid_path`.

---

## Performance notes

Per Rob Pike rule 2: we don't tune for speed until we've measured.
Things that look like they matter:

- **Copy buffer size**: `64 KiB` on the tar reader side matches
  typical disk readahead and is what most tooling uses.
- **zstd window**: default (8 MiB). Bumping it helps large layers
  compress better but costs memory. Measure before changing.
- **Directory walk during build**: `walkdir` crate, single-threaded.
  Parallel walks have synchronization cost and rarely help for the
  few-MB layers most packages produce. If importers start ingesting
  multi-GB filesystems we revisit.
- **Checksumming**: sha256 is the hot loop for `elu build`. The
  `sha2` crate has SIMD paths (`asm` feature on x86_64) that are
  ~2× faster. Enable that feature in `elu-store`'s Cargo.toml; no
  API change needed.

No parallel tar extraction in v1. Single-threaded streaming is
simple and fast enough for the workloads we know about. A future
parallel-stacker design would only pay off for extremely fat
layers, which is not a v1 concern.

---

## Non-goals

- **No overlayfs mounting.** See the PRD. Overlay is an output
  concern if it's ever needed.
- **No reflink / hardlink unpack modes.** See the PRD.
- **No multi-compression index.** The `diffs/` index is first-
  writer-wins per store.md.
- **No OCI interop** (import or export) in v1. The byte-level
  compatibility is preserved so a future OCI importer can use the
  same `LayerReader` unchanged.

---

## Testing strategy

- **Unit tests**: magic-byte sniffer over fixtures, path
  canonicalization helper over adversarial inputs, whiteout parser.
- **Integration tests (`tests/`)**:
  - Round-trip: `LayerBuilder` → zstd-encoded bytes → `FsStore` →
    `LayerReader` → verify content.
  - Reproducibility: build the same source tree twice and assert
    byte-identical tar output.
  - Stacking: build three layers with overlapping paths, stack in
    order, verify last-writer-wins.
  - Whiteouts: layer A writes `a/x`, layer B writes
    `a/.wh.x`, stack → `a/x` is gone.
  - Opaque whiteouts: layer A writes `a/x`, `a/y`; layer B writes
    `a/.wh..wh..opq`, `a/z`; stack → only `a/z`.
  - Path escape: craft a tar with `../etc/passwd` and assert the
    reader rejects it.
  - Symlink attack: layer A creates `a -> /etc`, layer B writes
    `a/hostname`, stack → the write goes to `staging/a/hostname`
    because the symlink is unlinked first.
- **Fuzz (if time)**: `cargo fuzz` target on `LayerReader` with
  arbitrary byte inputs, asserting no panic and bounded memory.
