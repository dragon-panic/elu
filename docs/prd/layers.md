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

What a layer blob is **not**:

- Not compressed internally. The blob stored in the CAS is plain tar.
  Compression is an I/O concern handled at transfer time by the
  registry transport, not a property of the content. Storing plain
  tar means identical content always produces identical hashes
  regardless of what compressor was available when the layer was
  built.
- Not encrypted.
- Not signed. Hash identity is the integrity story.

The hash of a layer is the hash of its plain-tar bytes.

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
- No tar+gzip requirement; the stored form is plain tar.
- No config blob separate from the manifest.

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
    for each layer_hash in layers:         # in manifest order
        layer_blob = store.get(layer_hash)
        for entry in tar_entries(layer_blob):
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

## Unpack Strategies

Stacking needs to move file contents from the store into the target.
The naive implementation reads the tar stream and writes new files.
elu supports three faster strategies when the filesystem allows:

| Strategy | Requires | Effect |
|----------|----------|--------|
| `copy` | always works | Read from store, write fresh bytes in target. |
| `reflink` | btrfs, xfs, apfs | Copy-on-write clone; zero data copy. |
| `hardlink` | same filesystem, target is read-only | Target entries are hardlinks into the store. No data copy; target cannot be modified. |

The strategy is chosen per-stack based on a policy:

```
--unpack=copy            # always copy
--unpack=reflink         # reflink if supported, else copy (default)
--unpack=hardlink        # hardlink, refuse if target will be mutated
```

Hardlink mode is attractive for read-only consumers (a runner pool
materializing skill stacks that will never be written back to). It is
not the default because a consumer that mutates the target would
corrupt the store.

Reflink mode is the default because it gives copy semantics at
hardlink cost on modern filesystems and degrades gracefully elsewhere.

The store is never modified by a stack operation. The store is a read
source; the target is the only thing written.

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
layers.stack(manifest, target, *, unpack_strategy="reflink", hooks=True)
    -> stats

# Apply a single layer (lower level, used by stack)
layers.apply(layer_hash, target, *, unpack_strategy="reflink")
    -> stats

# Compute the ordered layer list from a manifest + its resolved deps
layers.flatten(manifest, *, resolver) -> list of layer_hash
```

`flatten` walks dependencies depth-first and emits each dep's layers
before the declaring package's layers, deduplicating by hash so a layer
appearing in multiple branches is applied only once (in its first
position). The resolver (see [resolver.md](resolver.md)) produces the
pinned manifest graph that `flatten` walks.

---

## Non-Goals

**No overlayfs mounting.** elu materializes layers into real
directories. It does not create overlay mounts. Consumers that want
overlay semantics at runtime (e.g. a container runtime) can use
`unpack=hardlink` to avoid copying and do their own mount on top.

**No file-level deduplication within a layer.** Dedup happens at the
layer hash level. Two layers that share most of their files are two
distinct blobs. This is a tradeoff in favor of simplicity and against
maximum storage efficiency.

**No delta layers.** A layer is a complete tar stream. "Just the
changes since layer X" is expressible at the producer layer (by
authoring a layer that only contains the delta and whiteouts) and
does not need engine support.
