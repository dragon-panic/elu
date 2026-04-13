# Package Manifest

A package is a manifest plus the layer blobs it references. The manifest is
the single structured document that describes the package: what it is
called, what version it is, what kind of thing it represents, what layers
compose it, what it depends on, and what (if anything) should happen after
its stack is unpacked.

Manifests are stored in the content-addressed store alongside the layer
blobs they reference. A manifest's hash is the package's canonical
identity. Tags and names resolve to manifest hashes through the registry.

---

## Shape

The manifest is a structured document (TOML on disk, equivalent JSON on
the wire). Field names and types are stable; unknown fields are preserved
but ignored by elu itself so consumers can carry their own metadata
without a manifest version bump.

```toml
# manifest.toml
schema = 1

[package]
namespace   = "ox-community"
name        = "postgres-query"
version     = "0.3.0"
kind        = "ox-skill"
description = "Query PostgreSQL databases, inspect schemas, explain plans"
tags        = ["database", "postgresql", "observability"]

[[layer]]
hash = "b3:8f7a1c2e4d..."
size = 18432
name = "bin"          # optional, purely for humans / diagnostics

[[layer]]
hash = "b3:3b9e0a77f1..."
size = 512
name = "docs"

[[dependency]]
ref     = "ox-community/shell"
version = "^1.0"

[hook]
command = ["sh", "-c", "chmod +x bin/*"]

[metadata]
# Free-form, consumer-specific. elu preserves but never interprets.
homepage = "https://github.com/ox-community/postgres-query"
requires = { bins = ["psql"], network = ["*.postgres.example.com:5432"] }
```

---

## Fields

### `schema`

Required integer. The manifest schema version. elu refuses manifests
with a schema it does not understand. Bumped only when a
backward-incompatible change is made to field semantics. Adding new
optional fields does not bump the schema.

### `[package]`

**`namespace`** — required. The publisher namespace. Scoped at the
registry level; see [registry.md](registry.md). Lowercase, dash-separated.

**`name`** — required. Package name within the namespace. Lowercase,
dash-separated. The pair `namespace/name` is unique across the registry.

**`version`** — required. Semver string. The human-facing version. The
authoritative identity of the package is still its manifest hash; the
version exists to let humans and resolvers talk about releases.

**`kind`** — required. Opaque string. elu does not interpret it. Consumers
dispatch on it. The reserved value `native` means "an ordinary elu
package, no consumer-specific semantics, just unpack the layers." Any
other value is a contract between publishers and consumers. See
[consumers.md](consumers.md).

Suggested conventions (not enforced by elu):

| `kind` | Meaning |
|--------|---------|
| `native` | Default. Plain layer stack. |
| `ox-skill` | An ox skill package. ox-runner knows how to mount it. |
| `ox-persona` | An ox persona package. |
| `ox-workflow` | An ox workflow package. |
| `ox-runtime` | An ox runtime definition package. |
| `debian` | Produced by the apt importer. |
| `npm` | Produced by the npm importer. |
| `pip` | Produced by the pip importer. |
| `os-base` | A base operating system layer set for qcow2 output. |

**`description`** — required. One-line human description, used in
registry search and CLI listings.

**`tags`** — optional. List of free-form strings used for discovery.
Tags are never load-bearing — no consumer should dispatch on a tag. If a
behavior needs to be triggered by metadata, that is what `kind` is for.

### `[[layer]]`

An ordered list. Layer order is significant: layers are applied in the
order they appear in the manifest, earlier first. Each entry:

**`hash`** — required. Content hash of the layer, including the
algorithm prefix (e.g. `b3:...`). The hash is always taken over the
**uncompressed tar bytes**, regardless of what encoding the blob is
stored or transferred in. Two layers with identical uncompressed
content have the same hash even if one was gzipped and one was
zstd-compressed on disk. The blob must be present in the store or
fetchable from the registry before the package can be stacked.

**`compression`** — optional. One of `none`, `gzip`, `zstd`. Defaults
to `zstd`. Declares the encoding the blob uses when stored in the CAS
and served from the registry. Since `hash` is over the uncompressed
tar, `compression` is a transport and storage hint, not an identity
field. Two publishers that compress the same logical layer with
different algorithms produce the same `hash` but different on-disk
bytes; a store keyed only by hash will hold whichever form arrived
first (see [store.md](store.md)).

**`size`** — required. Byte size of the **uncompressed tar**, matching
the bytes the hash covers. Used for progress reporting and sanity
checks. Not the source of truth — the hash is. The compressed-on-wire
size is a transport detail and is not carried in the manifest.

**`name`** — optional. A short label shown in diagnostics. Has no
effect on unpacking.

Layers themselves are described in [layers.md](layers.md). The manifest
only names them.

### `[[dependency]]`

Optional. Other packages this package requires. Each entry:

**`ref`** — required. A `namespace/name` reference.

**`version`** — optional. A semver constraint. Defaults to `*` (any).
If the constraint is an exact hash (`b3:...`), the resolver treats it
as pinned and skips version resolution for this dependency.

Dependencies are stacked before the declaring package's own layers.
This means a skill that depends on `shell` gets `shell`'s layers first,
then its own. The resolver produces a flat, ordered layer list by
walking dependencies depth-first; see [resolver.md](resolver.md).

### `[hook]`

Optional. A single command to run after the full stack has been
unpacked into the staging directory, before the output is finalized.

**`command`** — required if `[hook]` is present. An argv-style list.
The hook runs host-side with the staging directory as its working
directory. The hook does **not** run inside a chroot, a container, or
a guest. It runs in the same environment as the `elu` process itself.

**`env`** — optional. A map of environment variables set for the hook
process. elu sets `ELU_STAGING` to the staging directory path
unconditionally.

**`timeout_ms`** — optional. Wall-clock timeout. Defaults to 60_000
(60 seconds). A hook exceeding the timeout fails the stack operation.

Per-package, not per-layer. The 90% case is "finalize the tree after
everything is in place" (e.g. `ldconfig`, `chmod +x bin/*`, generate a
combined index). Per-layer hooks are a reserved extension — adding them
later is an additive schema change and does not break existing
manifests.

Because the hook is host-side and has access to whatever the elu
process has access to, publishing a package with a hook is equivalent
to asking consumers to run a script. Consumers that care about trust
can refuse to run hooks, or run them in their own sandbox. See
[consumers.md](consumers.md) and [registry.md](registry.md).

### `[metadata]`

Optional. A free-form table. elu preserves it verbatim, exposes it to
consumers, and never reads it. Consumers use this to carry whatever
information their `kind` requires — a skill might put `requires.bins`
and `inputs` here, a persona might put `runtime` and `model`, an os
base might put architecture and kernel version. Because elu never
reads it, adding new consumer-side fields never requires an elu change.

---

## Identity

A package's canonical identity is the content hash of its manifest. Two
manifests with identical bytes are the same package. A lockfile is a
list of such hashes.

Because the manifest contains the hashes of its layers and its
dependencies, manifest identity transitively pins the entire stack. If
a layer blob changes, its hash changes, so the manifest that names it
changes, so the manifest hash changes. There is no way for "the same
package" to silently reference different content.

This is the property that makes `@1.0.0` safe: the registry's mapping
from `namespace/name@1.0.0` to a manifest hash is fixed at publish
time. Re-publishing under the same version is rejected. Mutating a
published version requires a new version.

---

## Validation

When a manifest enters the store (published, imported, or received
from a registry fetch), elu validates:

1. `schema` is a supported version.
2. `namespace` and `name` match the allowed character set.
3. `version` parses as semver.
4. `kind` is a non-empty string.
5. Each `[[layer]].hash` parses as a hash with a known algorithm.
6. Each `[[dependency]].ref` parses as `namespace/name` and `version`
   parses as a semver constraint or hash.
7. Each `[[layer]]` blob exists in the store **or** its hash appears
   in the fetch plan the resolver is about to execute. Stack operations
   fail if a referenced blob cannot be made present.
8. If `[hook]` is present, `command` is a non-empty argv list.

Validation failures reject the manifest. elu never silently repairs a
manifest.

---

## Example: Native Package

A minimal native package with one layer and no dependencies:

```toml
schema = 1

[package]
namespace   = "dragon"
name        = "hello-tree"
version     = "0.1.0"
kind        = "native"
description = "An example package containing a greeting file"

[[layer]]
hash = "b3:d2c4..."
size = 42
```

Stacking this package unpacks its single layer into the target and
does nothing else. This is the baseline elu experience; every other
kind is this plus consumer-side interpretation of the manifest.

---

## Example: Consumer-Specific Kind

The same shape, with `kind` set and `metadata` carrying the consumer's
expected fields:

```toml
schema = 1

[package]
namespace   = "ox-community"
name        = "postgres-query"
version     = "0.3.0"
kind        = "ox-skill"
description = "Query PostgreSQL databases"
tags        = ["database", "postgresql"]

[[layer]]
hash = "b3:8f7a..."
size = 18432

[hook]
command = ["sh", "-c", "chmod +x bin/*"]

[metadata.ox]
requires  = { bins = ["psql"], network = ["*.postgres.example.com:5432"] }
inputs    = { connection_url = { type = "secret" } }
```

ox-runner reading this manifest sees `kind = "ox-skill"`, knows how
to interpret `metadata.ox`, places `bin/` on PATH, injects secrets,
and assembles the skill index. elu itself did none of that — it
unpacked a layer and ran a hook.
