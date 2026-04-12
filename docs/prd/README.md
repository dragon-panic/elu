# elu

elu is a universal content-addressed layer engine. It stores file trees as
hashed layers, stacks them into materialized outputs, and ships them through
a lightweight registry. The design goal is to be the substrate underneath
any system that needs reproducible, composable, shareable bundles of files —
skills and personas for AI agent frameworks, runtime images for sandboxes,
system package sets, container-adjacent artifacts, or anything else that
can be expressed as "these files, in this order, with this metadata."

elu has no opinions about what its packages mean. A package is a manifest,
a list of content-addressed layers, and a free-form metadata bag. What a
package is *for* — a skill, a persona, a workflow, a VM base image, an apt
package mirror — is expressed through the `kind` field and interpreted by
the consumer, not by elu.

---

## Mental Model

A **package** is a manifest plus an ordered list of layers. A **layer** is
a content-addressed blob representing a file tree. A **stack** is a
resolved ordered list of layers materialized into a target: a directory,
a tarball, a qcow2 image. The **store** is the content-addressed object
database that holds manifests and layer blobs. The **registry** is a
lookup service that maps names and versions to manifest hashes.

```
                      ┌─────────────────┐
                      │    Registry     │
                      │  name → hash    │
                      └────────┬────────┘
                               │ resolve
                               ▼
┌──────────┐  stack   ┌─────────────────┐  materialize   ┌──────────┐
│ manifest │ ───────▶ │    Resolver     │ ─────────────▶ │  Output  │
└──────────┘          │ hashes in order │                │  tar/dir │
      ▲               └────────┬────────┘                │  qcow2   │
      │                        │                         └──────────┘
      │                        ▼
      │               ┌─────────────────┐
      └────────────── │      Store      │
        reference     │ blobs by hash   │
                      └─────────────────┘
```

The flow is always the same: name a package, resolve its manifest and
transitive dependencies to hashes, fetch any missing blobs into the
store, stack them in order, and emit an output.

---

## Why Content Addressing

Content addressing is load-bearing, not decorative:

- **Reproducibility.** A hash names exact bytes. `postgres-query@1.0.0`
  could be rebuilt or re-tagged, but the hash it resolved to at install
  time refers to the same tree forever.
- **Deduplication.** Two packages that share a common layer share
  storage and transfer cost. A runner pool serving dozens of skills
  downloads each unique layer once.
- **Integrity.** Fetching by hash is self-verifying. No signing
  infrastructure is needed for the basic "what you got is what was
  published" property.
- **Lockfiles are trivial.** The lock is the set of hashes. No
  elaborate resolver state, no transitive version drift between
  machines.

Tags and semver ranges exist for humans. Hashes exist for machines.

---

## Ring Model

elu is organized in rings, each useful on its own but amplifying the
others:

1. **Store** — content-addressed object storage. The foundation.
2. **Layers** — unpack and stack semantics. Turns blobs into trees.
3. **Manifest** — the package format. Names, versions, layers, kinds.
4. **Resolver** — takes references, produces hashes. Handles deps.
5. **Importers** — bridge external ecosystems (apt, npm, pip) into
   native elu packages.
6. **Outputs** — materialize a stack as tar, dir, or qcow2.
7. **Registry** — name → hash lookup across publishers.
8. **CLI** — the operator surface for all of the above.

A user who only needs local reproducible file trees stops at ring 6.
A user publishing shared packages reaches ring 7. Everything above the
store depends on the store; nothing below depends on anything above.

---

## Components

| Component | Doc | Purpose |
|-----------|-----|---------|
| Manifest format | [manifest.md](manifest.md) | Package shape: name, kind, tags, layers, hook |
| Content-addressed store | [store.md](store.md) | Object database for manifests and blobs |
| Layers | [layers.md](layers.md) | Unpack and stack semantics |
| Dependency resolver | [resolver.md](resolver.md) | References → pinned hashes |
| Importers | [importers.md](importers.md) | apt, npm, pip → elu packages |
| Output formats | [outputs.md](outputs.md) | tar, dir, qcow2 targets |
| Registry | [registry.md](registry.md) | Publish and fetch across publishers |
| CLI | [cli.md](cli.md) | Operator command surface |
| Seguro integration | [seguro.md](seguro.md) | qcow2 images for sandbox VMs |
| Consumers | [consumers.md](consumers.md) | How systems on top of elu use `kind` |

---

## Key Principles

**Content addressing is the only identity that matters.** Names and
versions are human-facing sugar. The store, the resolver, and every
output format key on hashes. A lockfile is a list of hashes. Two
packages with the same content are the same package.

**`kind` is opaque to elu.** A package carries a `kind` string in its
manifest. elu parses it, exposes it, and never dispatches on it.
Consumers — ox-runner, seguro provisioners, anything else — read
`kind` and decide what to do. elu itself has no skill-specific or
persona-specific or runtime-specific code paths.

**One post-unpack hook per package, host-side.** A package may declare
a single command to run once the full stack is assembled in the
staging directory. It runs host-side against the staging tree, not
inside any guest. Per-layer hooks and guest-side execution are
deliberately out of scope; we will revisit when a real use case forces
the issue. See [layers.md](layers.md).

**Importers produce ordinary packages.** An imported apt package is
the same shape as a hand-written one. No second manifest format, no
"special" registry entries, no second code path through the resolver.
The importer is a build-time tool; the output is plain elu. See
[importers.md](importers.md).

**The registry is a lookup service, not a host.** The registry maps
names and versions to manifest hashes (and the upstream fetch URL for
the underlying blobs). It does not store blobs itself. Sources of
truth are the content store (for bytes) and the publisher's
infrastructure (for availability). See [registry.md](registry.md).

**Outputs are targets, not formats.** tar, dir, and qcow2 are three
ways of asking "put this stack somewhere usable." They share the
resolver, the store, and the stacker. Adding a new output is
localized — it does not touch the store or the manifest. See
[outputs.md](outputs.md).

**No plugin system.** Not for kinds, not for outputs, not for
importers. elu's extension points are its published interfaces. If
a consumer needs behavior elu doesn't have, it reads the store
directly or wraps the CLI. Adding a plugin boundary is a tax on
every future change.

---

## What elu Is Not

**Not a container runtime.** elu produces filesystem trees and images.
It does not run processes, manage namespaces, or supervise lifecycles.
Consumers that need those things layer them on top.

**Not a replacement for apt / npm / pip.** The importers are bridges,
not competitors. elu does not track upstream security advisories, run
dependency solvers for language ecosystems with their own constraints,
or republish upstream content under its own authority.

**Not a build system.** A package is assembled from layers produced
externally (by importers, build scripts, hand-curated trees). elu does
not know how to compile anything. It does know how to package the
result.

**Not tied to any one consumer.** ox, seguro, and hypothetical future
users are all equal. The design of elu should never be adjusted to
privilege one consumer's needs over another's — if ox needs something
elu-specific, it belongs in ox, not in elu.

---

## Status

This directory holds product requirements. Implementation tracks these
documents via cx issues. Documents in this directory describe the
intended behavior and the interfaces that cross component boundaries;
they do not describe internal data structures or Rust types. Pseudocode
and HTTP sketches are used freely; real code is not.
