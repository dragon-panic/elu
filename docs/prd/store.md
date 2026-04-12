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
2. **Hash determines object.** A hash resolves to at most one byte
   sequence. Collisions are treated as bugs in the algorithm, not as
   possible outcomes.

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
        8f7a1c2e4d3b...   # blob or manifest, named by full hash
  manifests/
    <hash>                # symlink or small index entry → objects/...
  refs/
    <namespace>/
      <name>/
        <version>         # file containing the manifest hash
  tmp/
    <random>              # staging for atomic writes
  locks/
    gc.lock               # exclusive lock during GC
```

`objects/` is the only place bytes live. Everything else is a pointer.

**`objects/<algo>/<two-hex>/<rest-of-hex>`** holds every blob and
every manifest. The two-character prefix bucket is purely a filesystem
fan-out optimization; it has no semantic meaning. elu makes no
distinction at this layer between "this is a manifest" and "this is a
layer blob" — the distinction is in how the object is used, not how it
is stored.

**`manifests/`** is a convenience index listing known manifest hashes.
It is rebuildable from a full scan and is not authoritative. Treat it
as a cache.

**`refs/<namespace>/<name>/<version>`** maps a human reference to a
manifest hash. This is the local mirror of what the registry serves;
see [registry.md](registry.md). A ref file is one line: the manifest
hash. Nothing else.

**`tmp/`** holds in-progress writes. Objects are written here first
and atomically renamed into `objects/` once the hash is verified.

**`locks/gc.lock`** is taken exclusively by GC to prevent races with
writers that are about to publish a new root.

---

## Writing Objects

Writing an object is always three steps: stream to `tmp/`, hash
on-the-fly, rename into place.

```
put(bytes) -> hash:
    staging = tmp/<random>
    h = hasher()
    while chunk in bytes:
        write staging <- chunk
        h.update(chunk)
    close staging
    hash = h.finalize()
    dst = objects/<prefix>/<rest>
    if exists(dst):
        remove staging              # already have it; dedupe win
    else:
        rename staging -> dst
    return hash
```

The rename is the commit point. A crashed writer leaves an orphan in
`tmp/` which GC cleans on next run. A reader that sees an object in
`objects/` is guaranteed it is complete and its bytes hash correctly.

elu never verifies the hash of an object it is about to read. The
invariant is maintained on write; read-time verification is the job
of `elu fsck`, not the hot path.

---

## Reading Objects

Reading is a direct path computation:

```
get(hash) -> bytes | not_found:
    path = objects/<algo>/<prefix>/<rest>
    if not exists(path):
        return not_found
    return read(path)
```

Consumers that stream a layer out during unpacking read directly from
the object path. This is how the layer stacker stays zero-copy on
filesystems that support reflink — the stacker asks the store for the
path, not the bytes.

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
    live = set()
    for each ref in refs/**:
        manifest_hash = read(ref)
        live.add(manifest_hash)
        manifest = parse(get(manifest_hash))
        for layer in manifest.layer:
            live.add(layer.hash)
        for dep in manifest.dependency:
            transitively add dep's manifest + layers
    for path in objects/**:
        hash = hash_from_path(path)
        if hash not in live:
            remove(path)
    remove stale tmp/* files older than 24h
    release locks/gc.lock
```

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
# Low-level object access
store.put(bytes)         -> hash
store.get(hash)          -> bytes | not_found
store.path(hash)         -> filesystem path | not_found
store.has(hash)          -> bool
store.size(hash)         -> bytes | not_found

# Manifests
store.put_manifest(bytes) -> hash             # validates + stores
store.get_manifest(hash)  -> manifest | not_found

# Refs
store.put_ref(namespace, name, version, hash)
store.get_ref(namespace, name, version)       -> hash | not_found
store.list_refs([namespace], [name])          -> list of (ns, name, version, hash)

# Maintenance
store.gc()                -> stats
store.fsck()              -> list of errors  # re-hashes every object
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
