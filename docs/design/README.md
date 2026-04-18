# elu: Implementation Design

This directory is the **implementation plan** for elu. The PRDs in
[../prd/](../prd/) describe what elu *is* and what behavior it must
exhibit at its interfaces; this directory describes *how* we build
it — Rust types, crate boundaries, on-disk layouts, wire formats,
chosen libraries, concurrency model, error strategy, and testing
approach.

The split is deliberate: the PRD does not name any Rust type or
library, so it survives rewrites and reviews by non-implementers;
the design docs do, so implementers have a single place to read
before writing code.

Every load-bearing design choice is recorded with a one-sentence
rationale. Choices that are "the obvious default" are listed in the
[boring defaults table](overview.md#boring-defaults) in
`overview.md` rather than re-justified in each component doc.

---

## Read order

Start with [overview.md](overview.md) for the cross-cutting
decisions (workspace layout, crate graph, async boundary, error
strategy, platform support, MSRV, boring defaults, state inventory,
v1 scope). Then read the component docs in ring order; each one
lists its PRD counterpart at the top.

---

## Component designs

| Design doc | PRD | Purpose |
|---|---|---|
| [overview.md](overview.md) | [../prd/README.md](../prd/README.md) | Cross-cutting: workspace, crate graph, async boundary, platform scope, error strategy, boring defaults, state inventory, v1 scope |
| [store.md](store.md) | [../prd/store.md](../prd/store.md) | `elu-store`: CAS, hash types, atomic writes, diffs index, refs, GC, fsck |
| [layers.md](layers.md) | [../prd/layers.md](../prd/layers.md) | `elu-layers`: tar read/write, zstd/gzip, whiteouts, stacker, path safety |
| [manifest.md](manifest.md) | [../prd/manifest.md](../prd/manifest.md) | `elu-manifest`: Manifest types, TOML ↔ canonical JSON, validation, ManifestReader for GC |
| [hooks.md](hooks.md) | [../prd/hooks.md](../prd/hooks.md) | `elu-hooks`: declarative op interpreter, v1 scope, deferred `run`/approvals |
| resolver.md *(todo)* | [../prd/resolver.md](../prd/resolver.md) | `elu-resolver`: ref → manifest-hash, dep graph, flatten, lockfile |
| importers.md *(todo)* | [../prd/importers.md](../prd/importers.md) | `elu-importers`: apt/npm/pip adapters |
| outputs.md *(todo)* | [../prd/outputs.md](../prd/outputs.md) | `elu-outputs`: dir/tar/qcow2 materializers |
| registry.md *(todo)* | [../prd/registry.md](../prd/registry.md) | `elu-registry`: async HTTP client + minimal server |
| cli.md *(todo)* | [../prd/cli.md](../prd/cli.md) | `elu-cli`: clap structure, `--json` envelope, error codes |
| [authoring.md](authoring.md) | [../prd/authoring.md](../prd/authoring.md) | `elu-author`: build/init/check/explain/schema pipeline, error codes |
| seguro.md *(todo)* | [../prd/seguro.md](../prd/seguro.md) | qcow2 handoff surface |
| consumers.md *(todo)* | [../prd/consumers.md](../prd/consumers.md) | Read-only store-access patterns for consumer tools |

---

## Load-bearing decisions, at a glance

These are the decisions from which everything else in this
directory follows. Each is explained where it lives; the list here
is a navigational index.

- **Hash algorithm: sha256** (via `sha2`). OCI byte-compatibility.
  See [overview.md](overview.md#boring-defaults) and
  [store.md](store.md#hashing).
- **Sync core, async only at the registry edge.** Store, layers,
  manifest, hooks, resolver, outputs are sync. `elu-registry` is
  the one `tokio` crate. See
  [overview.md](overview.md#async-boundary).
- **The `run` hook op and capability-approval model are deferred
  to v1.x.** v1 ships only the ten declarative ops. See
  [hooks.md](hooks.md#v1-scope-and-the-deferred-surface).
- **Cargo workspace, one crate per ring.** Dependency direction is
  enforced by the build graph. See
  [overview.md](overview.md#workspace-layout).
- **Stored manifests are canonical JSON**, not TOML. `elu.toml`
  is the human-facing source; the CAS holds canonical JSON so the
  manifest hash is deterministic. See
  [manifest.md](manifest.md#canonical-json-for-the-manifest-hash).
- **Hash types live in `elu-store`.** `DiffId`, `BlobId`,
  `ManifestHash` are newtypes over a common `Hash` — separate
  types make invariants a compile-time property. See
  [store.md](store.md#hash-types).
- **Flat TOML files for any future approval state.** No SQLite in
  client-side crates; SQLite exists only inside the registry
  server. See [overview.md](overview.md#state-store).
- **Registry server is in scope for v1** (minimal: `axum` +
  `rusqlite` + disk blobs). To be detailed in `registry.md`.
- **Reproducible tar output** (zero mtime, zero uid/gid, sorted
  entries) is a v1 requirement, not a future enhancement. See
  [layers.md](layers.md#writing).

---

## What's next

The first batch of design docs (overview, store, manifest, layers,
hooks) is complete. The remaining eight docs — resolver,
importers, outputs, registry, cli, authoring, seguro, consumers —
will be written in a second pass once the first batch is reviewed.
Each will link back to its PRD and record component-specific
choices without re-litigating the ones above.
