# Layers: Unpacking and Stacking

A layer is a content-addressed blob representing a file tree. A stack is
an ordered list of layers applied one after another into a target
directory. Stacking is the operation that turns stored bytes back into a
usable filesystem.

Layers are the unit of reuse in elu. Two packages that share a common
layer share storage and transfer. Stacks are the unit of materialization:
any output format (tar, dir, qcow2 — see [outputs.md](outputs.md)) is a
stack plus a finalization step.

---

## Layer Format

A layer is a tar stream containing file entries. That is the whole
format. elu chose tar because it is the lingua franca of filesystem
bundles: every platform reads it, every importer produces it, every
output format consumes it.

Inside the tar:

- Regular files with content, mode, uid, gid, mtime.
- Directories with mode, uid, gid.
- Symlinks with target, mode, uid, gid.
- Hardlinks (optional — elu preserves them if the producer emits
  them but does not require them).
- Whiteouts (see below).

### Identity and compression

The **hash of a layer is the hash of its uncompressed tar bytes.**
This is the only identity that matters: it is what the manifest
records, what the resolver pins, what the signature covers.

The blob as **stored on disk** and as **served from the registry**
may be compressed — v1 supports `none`, `gzip`, and `zstd`. The
compression algorithm is declared per-layer in the manifest (see
[manifest.md](manifest.md#layer)) and is a transport/storage hint,
not part of the identity. Two publishers that ship the same logical
layer with different compressions produce the same layer hash but
different on-disk bytes. Deduplication at the store layer is by
hash, so only one encoding is kept; see [store.md](store.md) for the
first-writer-wins rule.

The rationale for a single hash (rather than the OCI diff_id plus
blob-digest split): we get the same "encoding can evolve without
changing identity" property with half the bookkeeping. The thing
we lose is deduplication across multiple simultaneous compressed
encodings of the same tar in a single store — a narrow case we are
content to re-fetch when it happens.

What a layer blob is **not**:

- Not encrypted.
- Not signed. Hash identity is the integrity story.

### Whiteouts

A later layer can delete a path present in an earlier layer. Following
the OCI convention, a whiteout is a file named `.wh.<basename>` in the
parent directory; it marks `<basename>` for removal during stacking.
An opaque whiteout (`.wh..wh..opq`) removes every entry in the directory
before this layer's entries are applied.

Whiteouts are consumed during stacking and never appear in the
materialized output.

### Not an OCI image

elu layers look very similar to OCI image layers because the
underlying problem is the same. They are not OCI layers:

- No media types, no JSON descriptors, no manifest lists.
- A single hash per layer (over the uncompressed tar), not OCI's
  diff_id plus blob digest pair.
- No config blob separate from the manifest.
- Whiteout convention (`.wh.foo`, `.wh..wh..opq`) is borrowed
  verbatim, so an elu → OCI bridge can rewrap layers mechanically.

Interop with OCI is a bridging concern. An OCI importer (future work)
could rewrap OCI layers as elu layers; the reverse would produce OCI
images from elu stacks. Neither is in v1.

---

## Stacking Semantics

Stacking takes an ordered list of layer hashes and a target directory
and produces a merged file tree.

```
stack(layers, target):
    ensure target exists and is empty (or enforce --force)
    for each layer_hash in layers:                    # in manifest order
        raw = store.open(layer_hash)                  # file handle
        tar = decompress_stream(raw, layer.compression)
        for entry in tar_entries(tar):
            apply entry into target
```

Applying an entry:

- **Regular file:** write at `target/<path>`, overwriting any existing
  entry at that path (later layer wins).
- **Directory:** create if absent, update mode/uid/gid if already
  present.
- **Symlink:** create, replacing any existing entry at that path.
- **Whiteout (`.wh.foo`):** delete `target/<parent>/foo` if it
  exists. The whiteout entry itself is not materialized.
- **Opaque whiteout (`.wh..wh..opq`):** delete every entry under
  `target/<parent>/` before applying this layer's entries in that
  directory.

Order is significant. Layer N sees the merged state of layers 0..N-1
and may add, replace, or remove entries from it. This gives the same
"last writer wins" semantics as OCI without pulling in the OCI
specification.

---

## Unpack Mechanics

Stacking is straightforward: open each layer blob from the store,
decompress it according to the layer's declared compression, walk the
tar entries, and write them into the staging directory. Later layers
overwrite earlier ones on path collision; whiteouts delete.

The store is never modified by a stack operation. The store is a
read source; the staging directory is the only thing written.

**No reflink, no hardlink strategy.** An earlier draft of this
document described `copy`, `reflink`, and `hardlink` unpack modes.
None of them survive contact with the real use cases:

- Reflink/hardlink require the store to hold individual files, not
  tar blobs. Our store holds (compressed) tar blobs, so the
  per-file link operations do not apply — they would require a
  second "extracted cache" tier whose maintenance cost is larger
  than the copy it saves.
- Our actual workloads — qcow2 image builds, live skill injection,
  dev iteration — are not reflink-bound. Copy from tar is fast
  enough on SSD.
- Consumers that genuinely need zero-copy sharing across many
  concurrent stacks are better served by a union mount, which is an
  output concern (see **Future: overlay output** below), not a
  stacker concern.

Plain copy from a decompression stream is the only unpack path. No
flags, no strategy selection, no per-filesystem capability probing.

### Future: overlay output

If a consumer ever needs "many read-only stacks sharing filesystem
pages" — the classic containerd snapshotter pattern — the right
answer is an `overlay` output format (see [outputs.md](outputs.md))
that extracts each layer to its own directory and exposes the stack
as a kernel overlayfs mount with the layers as stacked lowerdirs.
This would be additive to `dir`, `tar`, and `qcow2`, not a change to
the stacker itself. It is explicitly out of scope for v1; we are
listing it so nobody re-introduces reflink/hardlink in search of the
same property.

---

## Staging Directory

A stack is always assembled in a **staging directory** before it becomes
the final output. Staging is the workspace where layers are applied
and the post-unpack hook runs.

For a `dir` output, staging is a temporary directory that gets renamed
into the final path on success. For a `tar` output, staging is a
temporary directory that gets walked and streamed into a tar file. For
a `qcow2` output, staging is a temporary directory that gets copied
into a guest image. See [outputs.md](outputs.md).

The staging directory is always on the host filesystem. The post-unpack
hook sees it at the path given by `$ELU_STAGING`. Nothing about the
staging directory is visible to a guest or a container — it is a
plain host-side directory.

---

## Post-Unpack Hook

If the manifest declares a `[hook]` (see [manifest.md](manifest.md)),
it runs once after the full stack is assembled in staging and before
the output is finalized.

```
stack(manifest, target):
    staging = mkdtemp()
    for layer in resolved_layers(manifest):
        apply(layer, staging)
    if manifest.hook:
        run_hook(manifest.hook, cwd=staging, env=hook_env(staging))
    finalize(staging, target, output_format)
```

Hook execution:

- **cwd** is the staging directory.
- **env** starts from the elu process's environment, adds
  `ELU_STAGING=<staging path>`, and overlays the manifest's
  `hook.env` table if present.
- **stdin** is `/dev/null`.
- **stdout and stderr** are captured and reported; on success they
  are discarded, on failure they are included in the error.
- **timeout** defaults to 60 seconds, overridable via
  `hook.timeout_ms`.
- **exit code 0** means success; anything else fails the stack.

A hook that fails rolls back the entire stack operation: the staging
directory is removed, no output is produced. Partial outputs are
never committed.

### Trust boundary

The hook runs with the privileges of the elu process. Publishing a
package with a hook is equivalent to asking every consumer to run
your shell script. Consumers that distrust hooks have two options:

1. Refuse packages whose manifest contains a hook
   (`--no-hooks`; stack fails if a hook is present).
2. Run elu itself inside their own sandbox (container, VM, seguro)
   so the hook's reach is bounded by what the sandbox allows.

elu itself does not sandbox hooks. A fancy sandbox is the kind of
feature that looks great in a PRD and ships with holes. The policy is:
hooks are trusted code, and consumers who want isolation get it from
the environment elu runs in.

### Why per-package, not per-layer

A per-layer hook is strictly more expressive — hook H2 on layer L2 can
observe the partial state after L0, L1, L2 but before L3. No known use
case requires this. The overwhelming majority of package finalization
("chmod this, generate that, run ldconfig once") is a single step at
the end.

Per-layer hooks are an additive schema change. If a real use case
appears, the manifest gains a `[[layer.hook]]` block; existing
manifests without per-layer hooks continue to work unchanged. v1 does
not pay the complexity cost until the cost is justified.

### Why host-side, not guest-side

A guest-side hook (running inside a chroot, a container, or a qcow2
image) needs an execution environment. That environment is the thing
elu is helping you build in the first place. Running a hook inside
the thing-you're-building creates ordering problems (what if the
shell isn't installed yet?) and portability problems (does the host
have qemu-user for cross-arch?). Host-side hooks have none of these
issues: they see a plain directory and run with the tools the
operator already has.

Consumers who need guest-side finalization (e.g. running
`update-initramfs` inside a qcow2) can do it at the output stage —
the qcow2 output owns that concern. See [outputs.md](outputs.md).

---

## Interface Sketch

```
# Stack a manifest into a target directory
layers.stack(manifest, target, *, hooks=True) -> stats

# Apply a single layer (lower level, used by stack)
layers.apply(layer_hash, compression, target) -> stats

# Compute the ordered layer list from a manifest + its resolved deps
layers.flatten(manifest, *, resolver) -> list of (layer_hash, compression)
```

`flatten` walks dependencies depth-first and emits each dep's layers
before the declaring package's layers, deduplicating by hash so a layer
appearing in multiple branches is applied only once (in its first
position). The resolver (see [resolver.md](resolver.md)) produces the
pinned manifest graph that `flatten` walks.

---

## Non-Goals

**No overlayfs mounting in the stacker.** The core stacker produces
real directories, not mounts. An `overlay` output format may provide
union-mount semantics in the future (see above), but it is an output,
not a property of stacking.

**No reflink or hardlink unpack modes.** See "Unpack Mechanics"
above. Plain copy from decompression streams is the one unpack path.

**No file-level deduplication within a layer.** Dedup happens at the
layer hash level. Two layers that share most of their files are two
distinct blobs. This is a tradeoff in favor of simplicity and against
maximum storage efficiency.

**No delta layers.** A layer is a complete tar stream. "Just the
changes since layer X" is expressible at the producer layer (by
authoring a layer that only contains the delta and whiteouts) and
does not need engine support.

**No multi-compression storage.** The store keeps exactly one
encoding per layer hash (first writer wins). A consumer that happens
to fetch the same hash compressed differently later pays a small
re-fetch cost but never stores both. See [store.md](store.md).
