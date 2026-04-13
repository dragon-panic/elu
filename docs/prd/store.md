# Content-Addressed Store

The store is the object database that underlies everything else in elu.
It holds two kinds of objects: **manifests** and **blobs** (layer bytes).
Both are addressed by the hash of their content. Every other component —
the resolver, the stacker, the registry, the importers, the outputs —
reads and writes the store.

The store is intentionally boring. It is a hash-keyed filesystem
directory with an atomic-write discipline and a garbage collector. It
is not a database; it does not index; it does not query. Higher
components do any indexing they need on top.

---

## Addressing

Every object has a hash of the form `<algo>:<hex>`. The algorithm
prefix is part of the canonical identity so we can migrate hash
algorithms later without rewriting every reference.

v1 uses a single algorithm. The choice is a multi-hash-style prefix
(`b3:...` for BLAKE3 or `sha256:...` for SHA-256); the interface is
identical regardless of which algorithm is active. All manifests and
blobs written by a given elu version use the same algorithm; the store
can hold objects from multiple algorithms simultaneously during a
migration window.

Two invariants:

1. **Content determines hash.** Identical bytes have identical
   hashes. Consumers rely on this for deduplication.
2. **Hash determines bytes.** A hash resolves to exactly one byte
   sequence on disk: `hash(get(k)) == k` for every key in the store.
   The store is a pure CAS — no exceptions, no compression
   substitutions, no "logical" hashing that diverges from what the
   file actually contains. Collisions are treated as bugs in the
   algorithm, not as possible outcomes.

### Two IDs for a layer, one CAS

Layers have two hashes (see [manifest.md](manifest.md#diff_id-vs-blob_id)):

- **`diff_id`** — hash of the uncompressed tar. Stable identity.
  Lives in the manifest.
- **`blob_id`** — hash of the bytes as stored on disk and transferred
  on the wire. The CAS key.

The CAS is keyed on `blob_id`, full stop. Invariant 2 applies to
`blob_id`. A small secondary index, `diffs/<diff_id>`, maps the
logical identity to the blob actually in the store so the stacker
can go from "the manifest says I need this layer" to "open this
file." The index is not part of the CAS invariant — it can be
rebuilt by scanning blobs, decompressing each, and computing the
diff_id. It is an accelerator, not a source of truth.

Manifests do not have this split — they are always stored
uncompressed, so their diff_id and their blob_id are the same hash.
The distinction only matters for layer blobs.

---

## Layout

The store lives under a root directory. The default is
`$XDG_DATA_HOME/elu` (typically `~/.local/share/elu`); operators can
override via `$ELU_STORE` or `--store` on the CLI.

```
<store-root>/
  objects/
    b3/
      8f/
        8f7a1c2e4d3b...   # blob, named by its blob_id (hash of stored bytes)
  diffs/
    b3/
      cc/
        ccaa11b2...       # one-line file: blob_id of the blob with this diff_id
  manifests/
    <blob_id>             # symlink or small index entry → objects/...
  refs/
    <namespace>/
      <name>/
        <version>         # file containing the manifest blob_id
  tmp/
    <random>              # staging for atomic writes
  locks/
    gc.lock               # exclusive lock during GC
```

`objects/` is the only place bytes live. Everything else is a
pointer or an index.

**`objects/<algo>/<two-hex>/<rest-of-hex>`** holds every blob. Every
file in `objects/` has the invariant that hashing it yields its own
path name. This holds for layer blobs in whatever encoding was
received (compressed or not) and for manifests (always uncompressed).
The two-character prefix bucket is purely a filesystem fan-out
optimization; it has no semantic meaning.

**`diffs/<algo>/<two-hex>/<rest-of-hex>`** is the diff_id → blob_id
index. Each file is one line: the blob_id of a blob in `objects/`
whose uncompressed tar hashes to this diff_id. The index is populated
on `put_blob` and consulted on every stack operation. It can be
rebuilt by scanning `objects/` and decompressing each layer. It is
not part of the CAS — losing `diffs/` means "scan objects to
rebuild," not "data lost."

**`manifests/`** is a convenience index listing known manifest
blob_ids. It is rebuildable from a full scan and is not
authoritative. Treat it as a cache.

**`refs/<namespace>/<name>/<version>`** maps a human reference to a
manifest blob_id. This is the local mirror of what the registry
serves; see [registry.md](registry.md). A ref file is one line: the
manifest blob_id. Nothing else.

**`tmp/`** holds in-progress writes. Objects are written here first
and atomically renamed into `objects/` once the hash is verified.

**`locks/gc.lock`** is taken exclusively by GC to prevent races with
writers that are about to publish a new root.

---

## Writing Objects

Writing a manifest is straightforward — hash the bytes, rename into
place. Manifest bytes are uncompressed, so the diff_id and the
blob_id coincide:

```
put_manifest(bytes) -> blob_id:
    staging = tmp/<random>
    h = hasher()
    while chunk in bytes:
        write staging <- chunk
        h.update(chunk)
    close staging
    blob_id = h.finalize()
    dst = objects/<prefix>/<rest_of_blob_id>
    if exists(dst):
        remove staging              # already have it; dedupe win
    else:
        rename staging -> dst
    return blob_id
```

Writing a layer blob computes **both** hashes in one pass. The raw
bytes (compressed or not) go to disk and are hashed to produce the
blob_id. In parallel, a decompression stream runs on those bytes and
the decompressed output is hashed to produce the diff_id. The raw
bytes are what the CAS stores; the diff_id goes into the index:

```
put_blob(bytes) -> (diff_id, blob_id):
    staging = tmp/<random>
    blob_hasher = hasher()                       # over raw bytes as received
    diff_hasher = hasher()                       # over decompressed bytes
    decompressor = decompress_stream(sniff_magic(peek bytes))
    while chunk in bytes:
        write staging <- chunk                   # store what we got
        blob_hasher.update(chunk)
        for plain in decompressor.feed(chunk):
            diff_hasher.update(plain)
    for plain in decompressor.finish():
        diff_hasher.update(plain)
    close staging
    blob_id = blob_hasher.finalize()
    diff_id = diff_hasher.finalize()

    dst = objects/<prefix>/<rest_of_blob_id>
    if exists(dst):
        remove staging              # already have this exact encoding
    else:
        rename staging -> dst

    diff_path = diffs/<prefix>/<rest_of_diff_id>
    if not exists(diff_path):
        atomically write diff_path <- blob_id    # first-seen encoding wins
    # else: a prior encoding of the same logical layer is already
    # indexed; the new blob sits in objects/ but is not reachable via
    # diffs/ and will be GC'd unless something else references it.

    return (diff_id, blob_id)
```

The caller passes a bare byte stream; the writer sniffs magic bytes
to pick the decompressor. Magic bytes: plain tar has `ustar` at
offset 257; gzip starts with `1f 8b`; zstd starts with `28 b5 2f fd`.
These are unambiguous, so no separate compression parameter is
needed on the write call.

The rename is the commit point. A crashed writer leaves an orphan in
`tmp/` which GC cleans on next run. A reader that sees an object in
`objects/` is guaranteed it is complete and hashes to its filename.

elu never verifies the hash of an object it is about to read. The
invariant is maintained on write; read-time verification is the job
of `elu fsck`, not the hot path.

### What "first writer wins" means now

The CAS itself has no "first writer wins" behavior — different
blob_ids are different blobs, coexisting without conflict. The
first-writer-wins rule applies only to the `diffs/` index: if two
publishers ship the same source tree with different compressors,
their blobs produce the same diff_id but different blob_ids, and
only one (the first) gets indexed under `diffs/<diff_id>`. The
second blob is still in `objects/` — the CAS stored it correctly —
but is unreachable via the normal `diff_id → blob_id` lookup path.
GC will collect it unless some other path holds a reference.

The cost: duplicate fetch + transient disk use for the cross-
compression case. The benefit: a pure CAS invariant, no compound
keys, no (hash, compression) pairs anywhere in the data model.

---

## Reading Objects

Reading is a direct path computation. Manifests and layer blobs use
the same primitive — both are addressed by blob_id:

```
get(blob_id) -> bytes | not_found:
    path = objects/<algo>/<prefix>/<rest_of_blob_id>
    if not exists(path):
        return not_found
    return read(path)
```

To fetch a layer by its logical identity, the stacker first resolves
through the diffs/ index:

```
resolve_diff(diff_id) -> blob_id | not_found:
    path = diffs/<algo>/<prefix>/<rest_of_diff_id>
    if not exists(path):
        return not_found
    return read(path).trim()

get_layer(diff_id) -> file handle | not_found:
    blob_id = resolve_diff(diff_id)
    if blob_id is not_found: return not_found
    return open(objects/<prefix>/<blob_id>)
```

The consumer holds the file handle, streams the bytes through the
appropriate decompressor (picked via magic-byte sniff on the first
few bytes), and extracts the tar. The store does not decompress on
its consumers' behalf — it hands out raw files. See
[layers.md](layers.md) on the unpack flow.

---

## Refs

A ref is a named pointer at a manifest hash. Refs are the local cache
of the registry's authoritative mapping.

```
write_ref(namespace, name, version, hash):
    ensure refs/<namespace>/<name>/
    atomically write refs/<namespace>/<name>/<version> <- hash

resolve_ref(namespace, name, version) -> hash | not_found:
    path = refs/<namespace>/<name>/<version>
    if not exists(path):
        return not_found
    return read(path).trim()
```

Refs are append-only in practice: a ref for `v1.0.0` is written once
and never overwritten. Re-publishing the same version is rejected at
the registry layer and, if it ever occurred, would be caught here by
comparing the incoming hash to the existing one.

The store does **not** maintain a reverse index from manifest hash to
refs. If a consumer needs one, it scans `refs/`. This is a deliberate
simplification — refs are few, scans are cheap, and denormalizing
creates a class of consistency bugs we'd rather not own.

---

## Garbage Collection

GC reclaims objects no longer reachable from any ref. It is mark-and-
sweep and takes an exclusive lock so writes cannot race it.

```
gc():
    acquire locks/gc.lock
    live_blobs = set()              # blob_ids in objects/
    live_diffs = set()              # diff_ids in diffs/
    for each ref in refs/**:
        manifest_blob_id = read(ref)
        live_blobs.add(manifest_blob_id)
        manifest = parse(get(manifest_blob_id))
        for layer in manifest.layer:
            live_diffs.add(layer.diff_id)
            blob_id = resolve_diff(layer.diff_id)
            if blob_id is not none:
                live_blobs.add(blob_id)
        for dep in manifest.dependency:
            transitively add dep's manifest + layers
    for path in objects/**:
        blob_id = blob_id_from_path(path)
        if blob_id not in live_blobs:
            remove(path)
    for path in diffs/**:
        diff_id = diff_id_from_path(path)
        if diff_id not in live_diffs:
            remove(path)
    remove stale tmp/* files older than 24h
    release locks/gc.lock
```

GC walks each manifest's layer list, resolves each diff_id to its
blob_id via `diffs/`, and adds both to the live set. Blobs with no
reachable diff_id (e.g. the second-encoding leftovers discussed
above) are collected.

GC is never automatic. Operators run `elu gc` (see [cli.md](cli.md))
on a schedule or on demand. Never running GC is a valid operational
choice — the store grows but stays correct.

Pins outside of refs (e.g. in-memory stack operations, installed
outputs on disk) are **not** tracked by GC. A user who does
`elu stack foo -o /srv/foo` and then `elu gc` still has `/srv/foo`;
GC only touches the store. A user who extracts into the store's
own cache area and then runs GC may lose data. Consumers that need
"this hash must stay alive" use a ref.

---

## Concurrency

Multiple elu processes may share a store concurrently. The
guarantees:

- **Writes are atomic.** Two writers producing the same object
  converge safely because the destination path is hash-derived.
- **Reads are lock-free.** A reader sees either the full object or
  `not_found`, never a partial file.
- **Ref writes are atomic.** Implemented via write-to-tmp + rename.
- **GC is exclusive.** Writers observing `gc.lock` taken wait or
  fail loudly; they do not proceed past the commit point.

Writers **do not** need the GC lock. They take it only to insert a
new ref whose manifest transitively references objects that might be
mid-sweep — and in practice the easier rule is "hold gc.lock during
the whole publish of a new ref" which is what v1 does.

---

## Interface Sketch

The public surface of the store, in pseudocode:

```
# Low-level blob access (CAS keyed by blob_id)
store.get(blob_id)           -> bytes | not_found
store.path(blob_id)          -> filesystem path | not_found
store.has(blob_id)           -> bool
store.size(blob_id)          -> bytes | not_found

# Layer blobs (compute both IDs, index diff_id → blob_id)
store.put_blob(bytes)        -> (diff_id, blob_id)
store.resolve_diff(diff_id)  -> blob_id | not_found
store.has_diff(diff_id)      -> bool

# Manifests (blob_id == diff_id because uncompressed)
store.put_manifest(bytes)    -> blob_id            # validates + stores
store.get_manifest(blob_id)  -> manifest | not_found

# Refs
store.put_ref(namespace, name, version, blob_id)
store.get_ref(namespace, name, version)          -> blob_id | not_found
store.list_refs([namespace], [name])             -> list of (ns, name, version, blob_id)

# Maintenance
store.gc()                   -> stats
store.fsck()                 -> list of errors    # re-hashes every object
```

Higher components use only this interface. They do not touch the store
layout directly — a future store backend (S3, a database, a remote
blob service) should be swappable without breaking any consumer.

---

## Non-Goals

**Not a database.** No queries, no joins, no indices beyond the
trivial filesystem ones. Consumers that need rich lookup build their
own projections.

**Not versioned.** The store has no notion of "store version" other
than the hash algorithm prefix. A store written by a newer elu is
readable by an older elu as long as the objects themselves are
format-compatible at the consumer layer.

**Not encrypted.** Blobs are at-rest plain. Operators who need
encryption encrypt the backing filesystem. elu does not own key
management.

**Not distributed.** A store is one directory on one filesystem. Two
machines share content by running a registry between them (see
[registry.md](registry.md)); they do not share a store directly.
