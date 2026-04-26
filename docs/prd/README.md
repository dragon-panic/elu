# elu

elu is a universal package manager with a serious trust story. It stores
file trees as content-addressed layers, stacks them into materialized
outputs, ships them through a lightweight registry, and — most importantly
— gives consumers real control over what a package is allowed to do at
install time. The design goal is to be the substrate underneath any system
that needs reproducible, composable, shareable bundles of files — skills
and personas for AI agent frameworks, runtime images for sandboxes, system
package sets, container-adjacent artifacts — without inheriting the supply
chain attack surface that every existing package ecosystem has accumulated
through decades of "hooks are arbitrary shell commands" defaults.

elu has no opinions about what its packages mean. A package is a manifest,
a list of content-addressed layers, and a free-form metadata bag. What a
package is *for* — a skill, a persona, a workflow, a VM base image, an apt
package mirror — is expressed through the `kind` field and interpreted by
the consumer, not by elu.

elu has strong opinions about what packages are allowed to *do*. Install
hooks are a closed set of declarative filesystem operations implemented in
elu's own code, not arbitrary shell commands. A single `run` op exists as
an explicit escape hatch for packages that genuinely need to execute a
binary, and `run` is governed by a capability-based permission model
patterned on Claude Code's tool approval system: capabilities must be
declared up front, consumers grant them per manifest hash (not per
version), and upgrades that change a package's capability profile force
re-approval. This is the feature that makes elu a *package manager*, not
just a layer engine — and the feature that differentiates it from every
package manager that exists today. See [hooks.md](hooks.md).

elu is built to be used by **humans and agents**, with the same
surface. Authoring a new package is one file (`elu.toml`) at the
project root, edited in 60 seconds, built with one command (`elu
build`), and published with one more (`elu publish`). The authoring
file is declarative: you name the files that go into each layer and
elu handles the content hashing, tar construction, and manifest
generation. elu does not build your software — you run `cargo`,
`make`, or whatever you already use, and point elu at the resulting
files. Docker's conflation of build-and-package is the anti-pattern
we are explicitly not repeating. For agents generating packages
from a natural-language spec, every command has a `--json` mode
with structured errors and stable error codes, `elu schema` emits a
JSON Schema for offline validation, `elu init --from <dir>` infers
a starter from an existing project tree, and `elu explain` renders
plain-English package summaries for PR descriptions on lockfile
bumps. See [authoring.md](authoring.md).

---

## Mental Model

A **package** is a manifest plus an ordered list of layers. A **layer** is
a content-addressed blob representing a file tree. A **stack** is a
resolved ordered list of layers materialized into a target: a directory,
a tarball, a qcow2 image. The **store** is the content-addressed object
database that holds manifests and layer blobs. A **registry** is a
lookup service that maps names and versions to manifest hashes.
Registries are federated: there may be an elu-operated default, a
company-internal registry, a community registry, or a static mirror,
all speaking the same protocol.

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
| Manifest format | [manifest.md](manifest.md) | Package shape: name, kind, tags, layers, hook ops |
| Authoring workflow | [authoring.md](authoring.md) | `elu.toml` source format, `elu build`/`init`/`check`/`explain`/`schema` |
| Content-addressed store | [store.md](store.md) | Object database for manifests and blobs |
| Layers | [layers.md](layers.md) | Unpack and stack semantics |
| Hook capability model | [hooks.md](hooks.md) | Declarative ops, `run` escape, policy, manifest-hash approvals |
| Dependency resolver | [resolver.md](resolver.md) | References → pinned hashes |
| Importers | [importers.md](importers.md) | apt, npm, pip → elu packages |
| Output formats | [outputs.md](outputs.md) | tar, dir, qcow2 targets |
| Registry | [registry.md](registry.md) | Publish and fetch across publishers |
| CLI | [cli.md](cli.md) | Operator command surface |
| Seguro integration | [seguro.md](seguro.md) | qcow2 images for sandbox VMs |
| Consumers | [consumers.md](consumers.md) | How systems on top of elu use `kind` |

---

## Key Principles

**Safety is not a feature — it is the default.** Package install
hooks are a closed set of declarative filesystem operations
implemented in elu's own code, not arbitrary shell. Packages that
need to execute external binaries declare their capabilities up
front, consumers approve per manifest hash, upgrades that change the
capability profile force re-approval, and `trust` is never the
default policy. The supply chain attack vector that every existing
ecosystem has accumulated is closed by construction in the common
case and audited-and-gated in the escape case. See
[hooks.md](hooks.md).

**Authoring is cheap, for humans and for agents.** One file at the
project root. Build with one command. Publish with one more. No
ritual directory layouts, no multi-file configurations, no build
DSL. The `elu.toml` source format is the same schema as the stored
manifest with different fields populated; an author reading a
published manifest sees the same shape they wrote. Every CLI command
has a `--json` mode with stable error codes so an agent can iterate
on a generated file the same way a human does — validate, fix, re-
validate — without parsing prose. `elu init --from <dir>` infers a
working starter from an existing project tree, closing the "first
package" loop in a single command. See [authoring.md](authoring.md).

**elu does not build your software.** Building is whatever produces
the files (`cargo`, `make`, `npm`, a shell script, a human). elu
packages the files that already exist. This is a hard boundary:
every package ecosystem that conflated build and package — every
one — has paid for it in supply chain surface, debuggability, and
reproducibility drift. Docker made the mistake; we are not
repeating it. See
[authoring.md#what-elu-is-not-a-build-system](authoring.md#what-elu-is-not-a-build-system).

**Content addressing is the only identity that matters.** Names and
versions are human-facing sugar. The store, the resolver, and every
output format key on hashes. A lockfile is a list of hashes. Two
packages with the same content are the same package. Approval
decisions are keyed on those hashes too — an upgrade is a new hash,
which is a new approval moment.

**The protocol should survive the company.** elu may provide a
default hosted registry, but installability must not depend on one
cloud account, DNS name, or operator. Package formats, lockfiles,
registry records, and CAS verification rules are documented protocol
surfaces. Anyone should be able to run a compatible registry, mirror
public blobs, or preserve a locked package set because the durable
identity is the manifest hash and layer hashes, not the service that
first served them.

**`kind` is opaque to elu.** A package carries a `kind` string in its
manifest. elu parses it, exposes it, and never dispatches on it.
Consumers — ox-runner, seguro provisioners, anything else — read
`kind` and decide what to do. elu itself has no skill-specific or
persona-specific or runtime-specific code paths.

**Hooks are declarative, not shell.** A package's `[[hook.op]]`
entries select operations from a closed set (chmod, mkdir, symlink,
write, template, copy, move, delete, index, patch) implemented in
elu's own code. The single `run` op is the governed escape hatch
for executing binaries. Per-layer hooks and guest-side execution are
deliberately out of scope. See [hooks.md](hooks.md) for the full
capability model.

**Importers produce ordinary packages.** An imported apt package is
the same shape as a hand-written one. No second manifest format, no
"special" registry entries, no second code path through the resolver.
The importer is a build-time tool; the output is plain elu. See
[importers.md](importers.md).

**Registries are lookup services, not the system's root of trust.**
A registry maps names and versions to manifest hashes, exposes
publisher and policy metadata, and provides fetch hints for the
underlying blobs. It does not make bytes trustworthy by serving them.
The source of truth for content is the CAS identity, and availability
can come from registry URLs, mirrors, local stores, private seeders,
or verified peer transports. See [registry.md](registry.md).

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

## Relationship to OCI

Anyone who has worked with container images will notice that elu's
layer model is very close to OCI's. That is deliberate. OCI solved
the layered-filesystem-distribution problem well, and we are not
going to solve it better by being different. Where elu differs is
**what a package means**, not how layers are stored or transferred.

The shortest way to describe elu to someone who knows OCI:

> elu is OCI with the Image Config's runtime metadata
> (`entrypoint`, `cmd`, `env`, `workdir`, `user`, exposed ports)
> replaced with package-management metadata (`namespace`, `name`,
> `version`, `kind`, `[[dependency]]`, `[[hook.op]]`, `[metadata]`)
> and a capability-based trust model for install hooks that OCI
> does not have. Layers and the two-hash split are the same. The
> registry is thinner. That's the shape.

### Explicit mapping

| OCI concept | elu equivalent | Notes |
|---|---|---|
| Layer blob (tar, gzipped or zstd) | Layer blob | Same shape. |
| Layer's compressed digest | `blob_id` | CAS key. Hash of the stored bytes. |
| Layer's `diff_id` (uncompressed tar hash) | `diff_id` | Same concept, same name. The manifest records this one. |
| Image Config blob | **elu manifest** | Same role: stable identity, lists diff_ids, carries the metadata that survives recompression. Content differs — package fields instead of runtime-execution fields. |
| Image Config digest (= "image ID") | Manifest hash (= package identity) | Stable under recompression because both sides record only diff_ids, not blob digests. |
| Image Manifest (JSON, registry-facing, lists config digest + layer blob digests with media types) | Registry package record (the `layers` array mapping `diff_id → blob_id → url + sizes`) | Per-publication transport info. Our version is thinner — no media types, no config-vs-layer distinction, no base64-encoded nested blobs. |
| Image Index (multi-platform "fat manifest") | **not yet** — see below | Flagged as future work. |
| Registry (`/v2/.../manifests`, `/v2/.../blobs`) | elu registry HTTP API | Same shape. Simpler endpoints because we have fewer artifact types. See [registry.md](registry.md). |

### What we kept verbatim

- **The two-hash split.** `diff_id` (uncompressed tar hash) as the
  logical identity, compressed-blob hash as the CAS key. Exactly
  OCI's solution to "encoding should evolve without breaking
  identity." See [manifest.md](manifest.md#diff_id-vs-blob_id) and
  [store.md](store.md).
- **Whiteout convention.** `.wh.<name>` to delete, `.wh..wh..opq`
  for opaque directories. Byte-for-byte OCI compatible, so an elu
  → OCI bridge can rewrap layers mechanically. See
  [layers.md](layers.md).
- **Layer order semantics.** Later layer wins on path collision.
- **Content-addressed transport.** Every byte fetched is verified
  against a hash declared at a higher level. A compromised
  registry or blob host can only cause a failed fetch, never
  content substitution.

### What we dropped

- **Runtime execution fields.** `entrypoint`, `cmd`, `env`,
  `workdir`, `user`, `exposed_ports`, signal handling, health
  checks. elu packages files; elu does not run processes.
  Consumers (ox-runner, seguro) handle execution on top.
- **Media types.** OCI needs them because manifests reference
  multiple artifact types (config vs layer, compressed vs
  uncompressed, multi-platform manifests). We have one layer type
  and one manifest type; media types would add ceremony without
  expressive power.
- **OCI's manifest/config split as two distributed artifacts.**
  OCI ships both a manifest (registry-facing) and a config
  (identity-bearing) in the same push. We keep the package's
  identity-bearing document (the manifest) and move the
  transport-info document into the registry (where it's a lookup
  result, not a distributed artifact). One fewer blob to track.
- **The `history` array.** Optional in OCI, mostly useful for
  `docker history` ergonomics. elu's free-form `[metadata]` table
  can carry provenance if a publisher wants it; no first-class
  concept.

### What we added

- **`namespace/name/version` inside the manifest.** OCI puts these
  in registry tags, external to the image config. elu puts them in
  the manifest itself because a package's identity should include
  its intended name, not just its content. A renamed package is a
  different package.
- **`kind`.** An opaque string discriminator that consumers
  dispatch on. OCI has no equivalent — an OCI image is always "a
  container image." elu explicitly supports many consumer types
  (skills, personas, workflows, OS bases, user-defined); `kind`
  is how a consumer recognizes "this is for me." See
  [consumers.md](consumers.md).
- **`[[dependency]]` at the package level.** Packages can depend
  on other packages, which are resolved, stacked, and
  deduplicated transitively. OCI's only notion of "dependency" is
  "layers," which is intra-image. Cross-image deps are an
  external concern (`FROM` in Dockerfiles is a build-time hint
  that doesn't survive into the image). elu treats package-level
  deps as first-class, which is what makes it a package manager
  and not just an image format.
- **`[[hook.op]]` as declarative operations.** OCI images have
  no notion of install-time finalization at all; they rely on the
  underlying filesystem being correct when the tar is extracted,
  and everything else happens at container start. elu packages
  sometimes need finalization (chmod, symlink, generated index,
  template substitution) and expose it through a closed set of
  declarative ops. This is also the elu's trust story.
- **Capability-based trust model.** OCI has no answer for "what is
  this image's install script allowed to do" because OCI images
  don't have install scripts. Traditional package managers do have
  install scripts and have inherited every supply chain attack of
  the last twenty years as a result. elu sits in the middle: it
  can finalize like a package manager, but the finalization is a
  closed set of declarative ops implemented in elu's own code, with
  a governed `run` escape hatch whose capabilities must be declared
  up front and whose approvals are keyed on manifest hash. This is
  the central feature that differentiates elu from apt, npm, pip,
  cargo, and every other install-script-based ecosystem. See
  [hooks.md](hooks.md).
- **Free-form `[metadata]`.** OCI has `annotations` for the same
  purpose; the shape is different but the role is identical.

### What we haven't decided yet

- **Multi-platform.** OCI's image index solves "one tag → many
  per-arch manifests." We need an answer for importer-produced
  packages (`debian/curl` is arch-specific; so is any compiled
  binary). Two plausible shapes: encode arch in the version
  string (`@8.1.2-3+amd64`), or add an index artifact above
  manifests that maps `(os, arch) → manifest hash`. The latter
  is cleaner and matches OCI exactly; the former is simpler.
  Deferred until a real workload forces the choice. See
  [manifest.md](manifest.md).

### Why the resemblance

OCI got the hard parts right: content-addressed layers, the
diff_id/blob_id split, whiteout semantics, and a simple registry
transport with presigned blob URLs. Re-deriving any of these from first
principles would produce something nearly identical — and would
sacrifice the cheap bridge to OCI tooling that matters for
adoption.

The design goal of elu is to be to OCI what Cargo is to .tar.gz:
the same underlying format, dramatically better ergonomics at the
layer *above* the format, with a package manager's model of
identity, deps, metadata, **and the supply-chain trust story that
no existing package manager has meaningfully shipped.** Content
addressing plus capability-based install hooks plus manifest-hash-
keyed approvals is a combination that neither OCI nor apt/npm/pip
has, and it is the thing that makes elu worth building rather than
just using what already exists.

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
