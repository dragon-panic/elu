# Consumers

elu is a substrate. The packages it stores and stacks are *used* by
other systems — ox-runner dispatching a skill, seguro booting a VM,
a build pipeline assembling a release bundle. Those systems are
**consumers**. This document describes the contract between elu and
its consumers, the `kind` dispatch model, and how to add a new
consumer-side interpretation without touching elu itself.

The single most important rule in this document: **elu never
dispatches on `kind`.** A package of `kind = "ox-skill"` unpacks
exactly the same way as a package of `kind = "native"`. The
difference is what the consumer does with the unpacked tree, not
what elu does to produce it.

---

## The Dispatch Model

A consumer reads a package's manifest, looks at `kind`, and decides
what to do:

```
# consumer-side pseudocode
def handle_package(manifest_hash):
    manifest = elu.inspect(manifest_hash)
    match manifest.kind:
        case "native":
            # just files; use them
            stack_and_use(manifest)
        case "ox-skill":
            stack_and_register_as_skill(manifest)
        case "ox-persona":
            stack_and_register_as_persona(manifest)
        case "os-base":
            # handled by elu's qcow2 output, not a user consumer
            error("os-base is not a user-facing kind")
        case _:
            # unknown kind; treat as native or refuse
            fall_back(manifest)
```

The consumer is the one that knows what `ox-skill` means. elu does
not. A new consumer for a new `kind` is purely a change in the
consumer's code; elu ships it without modification.

### What the consumer receives

When a consumer asks elu to stack a package, it gets back:

1. A staging directory with the merged file tree.
2. The manifest, including `kind`, `description`, `tags`, and
   `metadata`.
3. The hash of the manifest (for lockfile use).

From there, the consumer interprets `metadata` according to its
`kind` contract and takes appropriate action — register the skill
on PATH, install the persona in a config directory, boot the
image, whatever.

---

## Reserved Kinds

elu reserves a small set of `kind` values for its own use and for
well-known consumers. Publishers who want to define a new kind
should avoid these:

| Kind | Reserved for | Notes |
|------|--------------|-------|
| `native` | elu | Default. Plain stack, no consumer semantics. |
| `os-base` | elu's qcow2 output | Has required `[metadata.os-base]` fields. |
| `debian` | apt importer | Produced by `elu import apt`. |
| `npm` | npm importer | Produced by `elu import npm`. |
| `pip` | pip importer | Produced by `elu import pip`. |

Reserved kinds are the only kinds elu itself reads or writes. Every
other `kind` is a pact between publisher and consumer.

---

## Example: ox-runner

ox-runner is one of the consumers elu is designed to serve well. It
handles four kinds: `ox-skill`, `ox-persona`, `ox-workflow`, and
`ox-runtime`.

### Skills

An `ox-skill` package has a manifest like:

```toml
[package]
namespace   = "ox-community"
name        = "postgres-query"
kind        = "ox-skill"
description = "Query PostgreSQL databases, inspect schemas, explain plans"
tags        = ["database", "postgresql"]

[[layer]]
hash = "b3:8f7a..."

[hook]
command = ["sh", "-c", "chmod +x bin/*"]

[metadata.ox]
requires = { bins = ["psql"], network = ["*.postgres.example.com:5432"] }
inputs = {
    connection_url = { type = "secret", description = "Postgres DSN" },
    max_rows       = { type = "string", default = "100" }
}
```

When ox-runner dispatches a step that references this skill:

1. ox-runner asks elu to stack the skill into a staging directory
   (via the CLI with `--format dir`, or via a library call if one
   exists).
2. elu resolves, fetches missing blobs, applies the layer, runs the
   hook, and reports success.
3. ox-runner reads `metadata.ox` from the manifest.
4. ox-runner places the staging directory's `bin/` on the runtime
   process's PATH.
5. ox-runner resolves the declared secrets from its own secret
   store and injects them as environment variables.
6. ox-runner adds the skill's description to the agent's prompt
   index so the agent knows it is available.

Steps 1 and 2 are elu. Steps 3-6 are ox-runner. elu does not know
that `bin/` is special, that secrets exist, or that prompt indices
are a thing.

### Personas

An `ox-persona` package carries a persona's markdown instructions
and a small amount of metadata:

```toml
[package]
namespace   = "ox-community"
name        = "careful-coder"
kind        = "ox-persona"
description = "Deliberate, test-first software engineer"

[[layer]]
hash = "b3:..."      # layer contains persona.md

[metadata.ox]
runtime = "claude"
model   = "sonnet"
skills  = ["ox-community/shell", "ox-community/web-search"]
```

ox-server reads the persona package at config reload time, loads
the persona markdown from the stacked tree, and makes the persona
available to workflows. `metadata.ox.skills` drives further
package resolution: each referenced skill is itself an elu package
that gets stacked alongside.

### Workflows

An `ox-workflow` package wraps a TOML workflow definition in an elu
package so it can be versioned, pinned, and shared. The layer
contains one file (`workflow.toml`); ox-server reads it after
stacking.

```toml
[package]
kind = "ox-workflow"

[[layer]]
hash = "b3:..."      # contains workflow.toml
```

There is very little metadata here because the workflow.toml file
itself is the whole interface. elu's job is versioning and
distribution; ox's job is everything else.

### Runtimes

An `ox-runtime` package bundles an agent CLI, its configuration
templates, and optionally its dependency closure. For example,
`ox-runtimes/claude` might depend on `debian/nodejs` and contain a
layer with the `claude` binary and runtime.toml.

```toml
[package]
kind = "ox-runtime"

[[dependency]]
ref = "debian/nodejs"

[[layer]]
hash = "b3:..."      # claude binary + runtime.toml
```

ox-server reads `runtime.toml` from the stacked tree to get the
runtime definition, and ox-runner invokes the `claude` binary at
the path the package provides. Pre-baking the runtime into a seguro
image means it is already present by the time a step is dispatched.

---

## Example: A New Consumer

Suppose someone wants to use elu to package **desktop applications**
— the files needed to run a Linux desktop app, declared as a single
reference. They invent a new kind:

```
kind = "desktop-app"
```

and a new consumer program that reads packages of that kind and
installs them into a launcher:

```
def install_desktop_app(ref):
    manifest_hash = elu_cli.resolve(ref)
    manifest = elu_cli.inspect(manifest_hash)
    assert manifest.kind == "desktop-app"
    elu_cli.stack(manifest_hash, "/opt/apps/" + manifest.package.name)
    write_desktop_entry(
        path = "/usr/share/applications/" + manifest.package.name + ".desktop",
        fields = manifest.metadata.desktop
    )
```

No elu change is required. The package publishes through the normal
registry. The consumer dispatches on `kind = "desktop-app"` and reads
`metadata.desktop` for its fields. elu delivered the files; the
consumer decided what they meant.

This is the property the design is optimizing for: consumers land
without asking elu for permission.

---

## Consumer Responsibilities

Consumers are responsible for:

- **Interpreting `kind` and `metadata`.** elu exposes both; the
  consumer decides what they mean.
- **Choosing whether to run hooks.** A consumer that distrusts
  publisher-provided hooks can set `--no-hooks` when stacking. elu
  itself has no trust model; the consumer decides.
- **Providing secret material.** elu does not handle secrets. If a
  package declares it needs a secret (via convention in `metadata`),
  the consumer produces it.
- **Resolving consumer-specific references.** Some metadata fields
  (like `metadata.ox.skills`) contain additional elu references. The
  consumer walks those and asks elu to stack them too. elu does not
  follow references inside `metadata`.
- **Enforcing network, filesystem, or process isolation.** elu
  produces files. If they need to run sandboxed, the consumer
  arranges the sandbox.

---

## elu Responsibilities

elu is responsible for:

- **Resolving references to manifest hashes.** Semver, lockfiles,
  registry queries.
- **Fetching and storing blobs.** Content-addressed, verified on
  write.
- **Applying layers to a staging directory.** Ordered, with
  whiteouts.
- **Running the one declared post-unpack hook.** Host-side, with a
  timeout, in the staging directory.
- **Producing the requested output format.** dir, tar, qcow2.
- **Exposing manifest metadata to consumers.** Via `elu inspect`
  and the `--json` output of other commands.

That is the entire contract. Nothing above this list is elu's
problem; nothing below it is the consumer's problem.

---

## Non-Goals

**No kind-specific code paths in elu.** A PR that adds `if kind ==
"ox-skill"` to any elu component should be rejected. The behavior
belongs in ox, not here.

**No plugin system.** elu does not load user code to handle custom
kinds. Consumers are separate programs that call elu as a subprocess
or (eventually) link against a library crate. They do not extend elu
from inside.

**No kind registry.** elu does not maintain an authoritative list of
valid kinds. Publishers pick kind strings; consumers interpret them.
Collisions are a social problem, not an engine problem. If two
ecosystems both pick `kind = "skill"` and their consumers disagree
about what it means, one of them will lose. elu does not adjudicate.

**No introspection API beyond the manifest.** Consumers cannot ask
elu "what are all the packages with kind X in the store"? — they can
scan refs themselves and inspect each one. A future indexing layer
could provide this, but it is above the engine.
