# Seguro Integration

Seguro is a QEMU-based sandbox that runs CLI coding agents in
isolated VMs. It needs bootable disk images with specific software
preinstalled — a base OS, agent runtimes (claude, codex, etc.),
skills, project tooling. Building those images by hand is painful;
rebuilding them when a skill updates is worse.

elu is the tool seguro uses to build those images. Seguro declares
what should be in an image as a set of elu package references; elu
resolves, stacks, and produces a qcow2. Seguro boots it.

This document describes the integration from elu's side: what
interfaces seguro depends on and what contracts elu provides. Seguro
itself is described in its own repository; nothing in elu is specific
to seguro's internals.

---

## Boundary

The boundary between elu and seguro is one filesystem artifact: a
qcow2 image produced by `elu stack ... --format qcow2`. Everything
above that line is seguro's concern; everything below is elu's.

```
               ┌──────────────────────────┐
               │     seguro                │
               │  VM lifecycle, networking, │
               │  proxies, TLS inspection   │
               └──────────────▲────────────┘
                              │ boots qcow2
               ┌──────────────┴────────────┐
               │     elu stack ... qcow2    │
               │  resolve, fetch, stack,    │
               │  produce bootable image    │
               └──────────────┬────────────┘
                              │ reads
               ┌──────────────┴────────────┐
               │     elu store + registry   │
               └───────────────────────────┘
```

elu does not know how seguro launches VMs, what transparent proxies
it inserts, or how it meters API tokens. Seguro does not know how
elu stores blobs, resolves versions, or composes layers. The qcow2
is the contract.

---

## What a Seguro Image Contains

A runner image is an elu stack with:

1. **A base OS.** Typically a `debian/*` or `alpine/*` package of
   `kind = "os-base"` carrying a minimal root filesystem, kernel,
   and init. Produced by the apt importer against a minbase package
   set, or imported from an OCI base image.

2. **Agent runtimes.** Packages containing `claude`, `codex`, or
   other agent CLIs. These are typically hand-authored packages that
   bundle a binary and its config.

3. **Skills.** `kind = "ox-skill"` packages the runner will expose to
   agents at execution time. Pre-baking popular skills into the image
   avoids a per-dispatch fetch.

4. **System tooling.** `git`, `jq`, `ripgrep`, language runtimes —
   whatever the runtime or skill ecosystems expect on PATH. These
   come from the apt importer.

5. **Seguro-specific configuration.** A small layer with files
   placed at known paths: a systemd unit that launches ox-runner on
   boot, a `/etc/seguro/` config directory, SSH keys, network
   allowlists. This is authored by whoever is standing up the seguro
   deployment.

The image is assembled with one `elu stack` invocation:

```
elu stack \
    debian/bookworm-minbase \
    debian/git debian/jq debian/ripgrep \
    ox-runtimes/claude \
    ox-community/shell ox-community/web-search \
    acme-corp/seguro-runner-config \
    -o runner.qcow2 --format qcow2 --base debian/bookworm-minbase
```

The resulting qcow2 is what seguro boots.

---

## Base Image Requirements

Seguro needs the image to boot into a known-good state. The base OS
package carries the metadata the qcow2 output needs:

```toml
[package]
namespace = "debian"
name      = "bookworm-minbase"
version   = "12.5"
kind      = "os-base"

[metadata.os-base]
arch       = "amd64"
kernel     = "linux-image-amd64"
init       = "systemd"
finalize   = ["update-initramfs", "-u"]
```

See [outputs.md](outputs.md) for how the qcow2 output consumes this.
Seguro does not talk to the base image directly; it goes through
elu, which goes through the qcow2 output, which reads the
`os-base` metadata.

---

## Warm Pools

Seguro operators keep a warm pool of pre-booted VMs ready to accept
dispatches. The image those VMs boot from changes rarely — a new
skill does not usually force a rebuild. elu supports this in two
ways:

**Layered images.** The base image is expensive to build; the skills
on top are cheap. Because elu uses content-addressed layers, a new
version of one skill only changes that skill's layer. The rest of
the stack's layers are unchanged, so rebuilding the image reuses them
from the store.

**Hash-pinned lockfiles.** Operators commit an `elu.lock` for their
runner image. Rebuilds with `--locked` produce byte-identical qcow2
images until the lockfile is updated. This is what lets a warm pool
exist: every VM in the pool is provably running the same bits.

**Incremental rebuilds.** Because the staging directory is cheap to
assemble (reflink where available), rebuilding a qcow2 after a skill
update is dominated by the qcow2 output step, not by unpacking
layers. A 2GB runner image typically rebuilds in seconds.

---

## Live Skill Injection

Some skills change often enough that baking them into the image is
the wrong move. For those, seguro mounts a secondary elu store into
the guest via virtiofs and has ox-runner resolve skills on dispatch,
stacking them into a scratch directory inside the VM at runtime.

This means:

- The VM image contains a minimal base plus the agent runtime.
- The host's elu store is exposed (read-only) into the guest.
- ox-runner inside the guest runs `elu stack <skills> -o
  /run/ox/skills --format dir` at dispatch time.
- Because the store is shared, no blob transfer happens — it is a
  local stack operation.

elu's only contribution here is the `dir` output and the offline
resolver. Neither is seguro-specific.

---

## What elu Does Not Handle

**VM lifecycle.** Starting, stopping, snapshotting, restoring VMs is
entirely seguro's job. elu exits once the qcow2 is on disk.

**Networking.** Seguro's transparent proxy, allow/deny lists, and
TLS inspection are outside elu's scope. The `requires.network`
field in a skill manifest is informational; seguro enforces it by
configuring its proxy, not by asking elu.

**Secret injection.** elu does not handle secrets. Seguro mounts
secret material into the VM (SSH keys, API tokens) at launch time.
A package manifest can declare what secret environment variables it
expects via `[metadata]`, but that is a contract between the package
and the consumer, not something elu reads.

**Ephemeral keys.** Seguro issues ephemeral SSH keys per VM. elu
knows nothing about this.

**Token metering.** Seguro's AI API token metering is not an elu
concern. An image built with elu contains the runtime that makes
the API calls; what seguro does with the traffic is above elu.

---

## Interface

Seguro's integration with elu is entirely through the CLI. There is
no seguro-specific API surface in elu, no shared library, no
protocol. A seguro image-build script looks like:

```
# Pinned inputs in elu.lock; --locked fails if drift
elu lock --locked

# Produce the image
elu stack \
    --locked \
    $(cat image-stack.txt) \
    -o build/runner.qcow2 \
    --format qcow2 \
    --base debian/bookworm-minbase

# Hand off to seguro's image distribution
seguro image publish build/runner.qcow2
```

That is the entire contract. Anything that improves this flow (new
output options, faster qcow2 materialization, richer importers) is
an elu change that seguro benefits from without needing any seguro
changes.

---

## Non-Goals

**No bidirectional integration.** elu has no notion of seguro. The
word "seguro" appears in this document and nowhere in the engine.
Removing seguro from the picture tomorrow — replacing it with
Firecracker, Kata, or plain qemu — is a seguro-side change; elu is
unaffected.

**No runtime coordination.** elu does not stream updates to running
VMs. An image is rebuilt, distributed, and re-booted to pick up
changes. Live patching is outside the scope of this integration.

**No image registry.** elu produces qcow2 files; where they go
afterward (object storage, content distribution, seguro's own
distribution mechanism) is up to the operator.
