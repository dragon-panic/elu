# Authoring: Making elu Packages

This document is about the author-facing workflow: how a human or an
agent gets from "I have some files" to "I have a working elu package
in the store, ready to publish." The manifest format itself is in
[manifest.md](manifest.md); this document is about the *source* file
that produces those manifests and the commands that consume it.

The design target: **first package in 60 seconds**, for both a human
typing at a terminal and an agent generating files from a natural-
language spec.

---

## What elu Is Not: A Build System

**elu does not build your software. It packages files that already
exist.**

This is the sharpest boundary in elu's design, and it is deliberate.
Every package ecosystem that conflated "build" and "package" — every
one — has paid for it in supply chain attacks, long build times,
confusing caching semantics, and bug reports that live at the
intersection of "my compiler is wrong" and "my packager is wrong."
Docker made this choice and it sucked: `Dockerfile` is a build script
and a packager stapled together, every `RUN` is arbitrary shell, and
debugging a failed image build means debugging a sequence of layers
produced by arbitrary commands that may or may not be
reproducible.

elu does not repeat this mistake. The separation is enforced:

- **Building** is whatever produces the files. `cargo build`,
  `make`, `npm run build`, `go build`, a shell script you wrote, a
  Nix expression, a Bazel rule, a human compiling by hand. elu does
  not care. elu does not run it. elu does not attempt to reproduce
  it.
- **Packaging** is what elu does. You hand elu a working directory
  containing the files you want packaged and an `elu.toml`
  describing how to organize them into layers, and elu produces a
  content-addressed manifest and the layer blobs behind it.

The typical flow:

```bash
# Build with whatever tool you already use:
make release
# or: cargo build --release
# or: npm run build

# Package with elu:
elu build
```

Two commands, two responsibilities. elu never invokes `make`.
`make` never knows about elu. If either one fails, the failure is
localized to one tool and debuggable in one way.

### Why strict

Consolidating build and package creates three problems we do not
want:

1. **Supply chain surface expansion.** If elu runs a build step,
   that build step is arbitrary code executed with the user's
   privileges, and every package that uses the build-step feature is
   a potential RCE. We solved hooks ([hooks.md](hooks.md)); we are
   not about to re-import the same problem through a different
   door.
2. **Responsibility confusion.** When a build fails inside Docker,
   half of the user's debugging effort goes into figuring out whether
   the problem is with the Dockerfile instructions, the base image,
   the builder state, the layer cache, or the actual build tool. A
   strict separation makes the failure domain obvious.
3. **Reproducibility drift.** A build tool that is not elu
   probably already has its own reproducibility story (Cargo's
   Cargo.lock, Nix's derivations, Bazel's hermetic builds). elu
   does not need to compete with them. Packaging is reproducible on
   its own once the inputs are fixed — same files + same elu.toml +
   same elu version = same manifest hash.

### What if a publisher really needs build-from-source?

They use their normal build tool, then point elu at the result. If
the build tool is itself not reproducible, that is a problem for the
build tool, not for elu. If a publisher wants to ship a package
whose distribution format is "source that gets built on install,"
they are asking for a different product than elu — that product is
Nix, and it already exists.

A future elu version **may** add a sandboxed, declaratively-described
build step, gated behind the same capability model that governs
`run` hooks. It is not in v1, and it is not on the roadmap. The
strict boundary holds until a compelling use case forces the issue.

---

## The `elu.toml` File

Every elu project has an `elu.toml` at its root. This file is the
source of truth for how a package is authored: its metadata, its
dependencies, its layers (declared as filesystem-to-layer mappings),
and its hook operations.

```toml
# elu.toml — committed to version control

schema = 1

[package]
namespace   = "dragon"
name        = "hello-tree"
version     = "0.1.0"
kind        = "native"
description = "A greeting tree with a binary and docs"
tags        = ["example"]

[[dependency]]
ref     = "ox-community/shell"
version = "^1.0"

# Layer 1: the compiled binary
[[layer]]
name    = "bin"
include = ["target/release/hello"]
strip   = "target/release/"
place   = "bin/"

# Layer 2: documentation
[[layer]]
name    = "docs"
include = ["README.md", "CHANGELOG.md", "docs/**/*.md"]
place   = "share/doc/hello/"

# Post-unpack finalization: make binaries executable
[[hook.op]]
type  = "chmod"
paths = ["bin/*"]
mode  = "+x"
```

This file is:

- **Committed to VCS.** It is source, not a build artifact.
- **Readable top-to-bottom.** No magic ordering, no hidden state.
- **One file.** Not a directory of config files, not a `build/`
  subdirectory with nested rules.
- **The same schema as the stored manifest**, with different fields
  populated. See [Source vs Stored Form](#source-vs-stored-form)
  below.

### Layer authoring fields

A source `[[layer]]` entry declares *how to assemble the layer from
files in the project*, not the resulting content hash. The fields
are build-time directives that `elu build` consumes and replaces
with `diff_id` + `size` in the stored manifest.

| Field | Required | Meaning |
|-------|----------|---------|
| `name` | no | Human label shown in diagnostics. No effect on packaging. |
| `include` | **yes** | Glob patterns rooted at project root. Files matching these patterns are candidates for inclusion in this layer. |
| `exclude` | no | Glob patterns filtering out files matched by `include`. Applied after `include`. |
| `strip` | no | Path prefix to remove from every included file before placement. |
| `place` | no | Path prefix under which stripped files are placed in the layer. Default: layer root. |
| `mode` | no | Default Unix mode for files in this layer if the source file has no mode. Default: `0644` for regular files, `0755` for directories. |
| `follow_symlinks` | no | Bool. If `true`, symlinks are followed and the target content is packed. If `false` (default), symlinks are preserved as symlinks in the layer. |

### Include is always opt-in

`include` is never `"."` by default and `elu.toml` does not have a
"pack everything" escape. You have to name what you want. This is
deliberate: packaging the entire project root by default is how
`.env` files, credentials, build caches, and `.git/` directories
accidentally end up in published packages. Every existing package
manager has war stories about this. elu makes the safe thing the
default by making the unsafe thing require typing.

If you genuinely want everything under a directory: `include =
["target/release/**"]`. That is explicit, and anyone reading the
manifest can see it.

### Warnings for sensitive patterns

`elu build` warns (not fails) when a layer's resolved file list
contains paths matching common sensitive patterns:

- `.env`, `.env.*`
- `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*`
- `.aws/credentials`, `.ssh/*`
- `.netrc`, `.git/**`
- Files matching `.gitignore` patterns (fuzzy match)

The warning names the file and the matched pattern. `--no-warn`
suppresses the warning; `--strict` promotes warnings to errors. CI
configurations typically use `--strict` to make accidental secret
inclusion a build failure rather than a publish failure.

### Glob syntax

Include, exclude, strip, and place patterns use standard glob
syntax — the same as [hooks.md](hooks.md) and gitignore:

| Pattern | Matches |
|---------|---------|
| `*` | Anything within a single path segment. |
| `**` | Anything across path segments. |
| `?` | A single character. |
| `[abc]` | One of `a`, `b`, `c`. |
| `{a,b}` | Alternation — `a` or `b`. |

Patterns are matched against paths relative to the project root.
Absolute paths in `include` are rejected. `..` segments are
rejected.

### Layer ordering

Layers are applied in the order they appear in `elu.toml`, which is
also the order they appear in the stored manifest. Later layers
overwrite earlier ones on path collision (see
[layers.md](layers.md)). An author who wants a specific override
semantic orders their layers accordingly.

---

## The Build Pipeline

`elu build` is the one command every author runs. It consumes an
`elu.toml`, produces layer blobs, writes a stored manifest to the
CAS, and returns the manifest hash.

```
elu build:
  1. Parse elu.toml from the current directory (or --manifest <path>).
  2. Validate that the source form is complete and consistent:
     - Required fields present.
     - Every layer has include patterns, no layer has diff_id.
     - No unknown fields (agents catch typos early).
  3. Resolve dependencies via the resolver (see resolver.md).
     Lockfile is updated unless --locked is passed.
  4. For each [[layer]], in declaration order:
       a. Walk the filesystem, collecting files matching include
          patterns, removing files matching exclude.
       b. Apply strip / place to compute the in-layer path of each file.
       c. Warn on sensitive-pattern matches.
       d. Build a tar stream in sorted path order, with uid/gid 0,
          mtime 0, and per-file mode from the filesystem (or the
          layer default).
       e. Pipe the tar through store.put_blob() to obtain
          (diff_id, blob_id).
       f. Record diff_id and uncompressed size.
  5. Validate hook ops:
     - Op types are known.
     - Referenced paths appear in at least one produced layer
       (or in a dependency's layer) for chmod / delete / etc.
     - run ops have all required capability declarations.
  6. Build the stored manifest with resolved layer entries.
  7. Write the manifest to the store via store.put_manifest().
  8. Print the manifest hash (or JSON if --json).
```

### Reproducibility

`elu build` is deterministic: same `elu.toml` + same file contents
+ same elu version = same manifest hash, byte-for-byte. The
reproducibility rules:

- Tar entries are written in sorted path order.
- Uid and gid are zero. Ownership is a deployment concern, not a
  packaging concern.
- Mtime is zero. The layer does not carry timestamps — a layer's
  identity is its content, not when it was built.
- Mode comes from the filesystem unless the layer specifies a
  default.
- Compression is deterministic for the chosen algorithm (v1
  default: zstd, level 3, no dictionary).

Two authors building the same project on two machines should
produce the same manifest hash. This is the property that makes
reproducible-builds workflows meaningful.

### Lockfile interaction

`elu build` resolves dependencies on every invocation and updates
`elu.lock` if the resolution changes. `--locked` refuses to
proceed if the lockfile would change — the CI mode.

On success, the lockfile contains every pinned dependency and every
`[package.hook_approval]` block needed to install the result. A
published package ships with its lockfile committed so downstream
consumers get the same pinned transitive closure.

### Watch mode

`elu build --watch` re-runs the build pipeline whenever a file
matching any layer's `include` patterns changes. Useful during
iteration: edit your binary's source, rebuild with cargo, and the
elu manifest rebuilds automatically in the background. Output is
incremental — only layers whose file contents changed are
repacked.

---

## `elu init`: Starting a New Package

The first-impression command. `elu init` creates an `elu.toml` in
the current directory (or a specified subdirectory) from a template.

```
elu init                                     # interactive
elu init --kind native --name my-pkg         # non-interactive, minimal
elu init --kind ox-skill --name my-skill     # ox-skill skeleton
elu init --kind ox-persona --name reviewer   # persona skeleton
elu init --from ./existing-dir               # infer from an existing directory
elu init --template ox-community/rust-skill  # from a registry template
```

### Templates

A template is an `elu.toml` skeleton plus any seed files, bundled as
a package of `kind = "elu-template"` in the registry (or shipped
locally in elu itself for built-in templates). Templates are
published the same way any other package is.

| Template | Produces |
|----------|----------|
| `native` (built-in) | Minimal `elu.toml` with one layer declared as `include = ["**"]` (commented out) and no hook. |
| `ox-skill` (built-in or registry) | `elu.toml` with `kind = "ox-skill"`, a `bin/` layer, a `chmod +x bin/*` hook op, and a placeholder `metadata.ox` block. |
| `ox-persona` | `elu.toml` with `kind = "ox-persona"` and a `share/ox/personas/` layer for the persona markdown. |
| `ox-runtime` | `elu.toml` with `kind = "ox-runtime"` and a runtime.toml scaffold. |

Registry templates are versioned, pinned, and auditable the same
way any other package is. A user or org can publish their own
templates to seed team-specific conventions:

```
elu init --template acme-corp/internal-service-skill
```

### `--from`: infer from a directory

Given an existing project tree, `elu init --from` produces a
best-guess starter `elu.toml`. The inference logic looks for
characteristic files:

| Signal | Guess |
|--------|-------|
| `Cargo.toml` | Rust project. Include `target/release/<crate-name>` as a `bin` layer. |
| `package.json` | Node project. Include `dist/` as a `lib` layer. |
| `pyproject.toml` | Python project. Include `dist/*.whl` contents. |
| `go.mod` | Go project. Include the main binary from the default build location. |
| `Makefile` | Generic build. Include a placeholder `bin/` layer with a TODO comment. |
| `README.md`, `CHANGELOG.md` | Add a `docs` layer including them. |

The output is never silent: every inferred field has a comment
explaining why it was guessed and what the author should verify
before running `elu build`.

```toml
# Generated by `elu init --from .` on 2026-04-12
# Review and edit before running `elu build`.

schema = 1

[package]
namespace   = "TODO-your-namespace"
name        = "hello-tree"                  # inferred from Cargo.toml
version     = "0.1.0"                       # inferred from Cargo.toml
kind        = "native"                      # best guess — change to ox-skill etc. if applicable
description = "TODO: one-line description"

[[layer]]
name    = "bin"
# Inferred from Cargo.toml. Run `cargo build --release` before `elu build`.
include = ["target/release/hello-tree"]
strip   = "target/release/"
place   = "bin/"

[[layer]]
name    = "docs"
include = ["README.md", "CHANGELOG.md"]
place   = "share/doc/hello-tree/"

[[hook.op]]
type  = "chmod"
paths = ["bin/*"]
mode  = "+x"
```

This is the killer feature for the agent flow: an agent can run
`elu init --from .` on a project directory and get a working
starter that it can then refine with focused edits, rather than
trying to generate the whole file from scratch.

---

## `elu check`: Validate Without Building

```
elu check                 # validate elu.toml against the schema
elu check --json          # structured output
elu check --strict        # fail on warnings as well as errors
```

`elu check` parses `elu.toml`, validates the schema, resolves
dependencies (offline if possible), walks include globs to check
they match at least one file, and runs the hook op pre-checks. It
does not build layer blobs.

Fast feedback. An agent iterating on a generated elu.toml runs
`elu check --json` after each edit and reads the structured errors
without waiting for a full build.

### Structured errors

All author-side commands support `--json`. On validation failure,
output is a JSON document with a stable shape:

```json
{
  "ok": false,
  "errors": [
    {
      "field": "layer[0].include",
      "code":  "no-matches",
      "message": "include pattern 'target/release/hello' matched zero files",
      "hint":  "Run the build step (e.g. `cargo build --release`) before `elu build`, or correct the include pattern.",
      "file":  "elu.toml",
      "line":  12
    }
  ],
  "warnings": []
}
```

| Field | Meaning |
|-------|---------|
| `ok` | Overall success bool. |
| `errors` | List of blocking errors. Empty if `ok` is true. |
| `warnings` | List of non-blocking warnings. |
| `errors[].field` | Dotted path to the offending elu.toml field. |
| `errors[].code` | Stable enum value, greppable and machine-dispatch-friendly. |
| `errors[].message` | Human-readable explanation. |
| `errors[].hint` | Actionable suggestion. |
| `errors[].file` / `line` | Source location for editors. |

Error codes are documented and stable across minor versions. Agents
dispatch on `code`; humans read `message` and `hint`.

---

## `elu explain`: Human and Agent Package Summaries

```
elu explain ox-community/postgres-query@0.3.2
elu explain b3:8f7a...
elu explain --json my-local-pkg
```

Prints a plain-English summary of what a package is and what it
does. Used during package review, agent decision-making, and
lockfile-bump PR inspection.

Example output:

```
ox-community/postgres-query @ 0.3.2
  manifest: b3:8f7a1c2e4d...
  kind: ox-skill
  publisher: ox-community (verified)

Purpose
  Query PostgreSQL databases, inspect schemas, explain query plans.

Layers (2)
  bin   — 18.4 kB   14 files (bin/pg-query, bin/pg-schema, bin/pg-explain, ...)
  docs  —    512 B    1 file  (share/doc/postgres-query/README.md)

Dependencies (1)
  ox-community/shell @ ^1.0    → 1.1.0 (b3:3b9e...)

Hook operations (2)
  1. chmod bin/* +x
  2. run: ["ldconfig"]
     reads:   lib/**
     writes:  lib/**, var/ld.so.cache
     network: false

Consumer metadata
  metadata.ox.requires: { bins: ["psql"], network: ["*.postgres.example.com:5432"] }
  metadata.ox.inputs:   connection_url (secret), max_rows (string, default "100")
```

The `--json` form emits the same information as a structured
document for programmatic consumption.

`elu explain` is what an agent calls when a lockfile is about to
bump a package — the agent renders the explain output into a PR
description, or uses it to decide whether to approve a hook that
changed since the previously-approved version.

---

## `elu schema`: Machine-Readable Schema

```
elu schema                  # JSON Schema for elu.toml (source form)
elu schema --stored         # JSON Schema for the stored manifest form
elu schema --yaml           # YAML Schema equivalent
```

Emits a JSON Schema document describing the expected shape of
`elu.toml`. Agents load this once and validate generated files
against it before running `elu build`, closing the feedback loop
without requiring elu to be in the path.

The schema is versioned alongside elu; `elu schema --version 1`
pins to a specific manifest schema version if an agent is
generating files for an older elu.

---

## Source vs Stored Form

A single `elu.toml` schema describes two related shapes:

- **Source form** — what the author edits and commits. Layer
  entries have `include` (required), `exclude`, `strip`, `place`,
  `mode`. No `diff_id`, no `size`. Used by `elu build` to produce
  a package.
- **Stored form** — what `elu build` writes to the CAS. Layer
  entries have `diff_id` (required), `size`, `name`. No `include`,
  no `strip`, etc. This is the package identity; its hash is the
  package's identity.

Both are valid `elu.toml` per the schema. The validation rule is:

> A `[[layer]]` entry must have **either** source-form fields
> (with `include` required) **or** stored-form fields (with
> `diff_id` required). Mixing is rejected. Build consumes
> source-form and emits stored-form.

The design intent: an author reading a stored manifest sees the
same structure they're used to from writing source, minus the
build directives. An agent generating a source file can use the
same schema validation path the stored manifest goes through.

### Why same file name

Calling both documents `elu.toml` (one at the project root, one in
the store) means:

- One schema file (`elu schema`) covers both forms.
- `elu inspect` can display the stored form as if it were a source
  file someone wrote, which is what most users expect — "show me
  the manifest" should look familiar.
- Authoring tools (editor integrations, language servers) only
  need to know one format.

The cost is a slight asymmetry at the field level. That cost is
smaller than the cost of learning two related-but-different schemas
and remembering when to use which.

---

## Worked Examples

### Example: a native package

```toml
schema = 1

[package]
namespace   = "dragon"
name        = "tree"
version     = "1.0.0"
kind        = "native"
description = "Prints a tree of files"

[[layer]]
name    = "bin"
include = ["target/release/tree"]
strip   = "target/release/"
place   = "bin/"

[[hook.op]]
type  = "chmod"
paths = ["bin/*"]
mode  = "+x"
```

Build:

```bash
cargo build --release
elu build
```

### Example: an ox-skill

```toml
schema = 1

[package]
namespace   = "ox-community"
name        = "web-search"
version     = "0.2.0"
kind        = "ox-skill"
description = "Search the web and fetch pages"
tags        = ["web", "research"]

[[dependency]]
ref     = "ox-community/shell"
version = "^1.0"

[[layer]]
name    = "bin"
include = ["dist/web-search", "dist/fetch-url"]
strip   = "dist/"
place   = "bin/"

[[layer]]
name    = "instructions"
include = ["SKILL.md"]
place   = "share/ox/skills/web-search/"

[[hook.op]]
type  = "chmod"
paths = ["bin/*"]
mode  = "+x"

[metadata.ox]
requires = { network = ["*"] }
inputs = {
    search_api_key = { type = "secret", description = "Search provider API key" }
}
```

### Example: an ox-persona

```toml
schema = 1

[package]
namespace   = "dragon"
name        = "careful-coder"
version     = "1.0.0"
kind        = "ox-persona"
description = "Deliberate, test-first software engineer"

[[layer]]
name    = "persona"
include = ["persona.md"]
place   = "share/ox/personas/careful-coder/"

[metadata.ox]
runtime = "claude"
model   = "sonnet"
skills  = ["ox-community/shell", "ox-community/web-search"]
```

No hook at all — the persona is pure data.

### Example: a package with `run`

A package that needs to strip debug symbols from its binary during
packaging, using an external tool:

```toml
schema = 1

[package]
namespace   = "dragon"
name        = "slim-tool"
version     = "2.0.0"
kind        = "native"
description = "A tool, slimmed"

[[layer]]
name    = "bin"
include = ["target/release/slim-tool"]
strip   = "target/release/"
place   = "bin/"

[[hook.op]]
type  = "chmod"
paths = ["bin/*"]
mode  = "+x"

[[hook.op]]
type    = "run"
command = ["objcopy", "--strip-debug", "bin/slim-tool"]
reads   = ["bin/**"]
writes  = ["bin/**"]
network = false
```

The `run` op is visible in `elu inspect`, declared in the
manifest, and will trigger an approval prompt on install unless
the consumer has a policy rule allowing it.

---

## The 30-Second Human Experience

```bash
# 1. Start a project
cargo new --bin my-tool
cd my-tool

# 2. Build
cargo build --release

# 3. Scaffold elu
elu init --from .

# 4. Inspect the generated elu.toml. Edit if needed.
$EDITOR elu.toml

# 5. Build the package
elu build

# 6. Done. Package is in the store.
elu inspect my-tool

# Optional: publish
elu publish my-tool@0.1.0
```

Total: six commands, about a minute including the edit pass. The
only file the user had to write from scratch is the one-line
description; everything else was inferred or defaulted.

## The Agent Experience

An agent generating packages follows the same path with different
ergonomics:

```
1. Agent loads elu-schema.json (from `elu schema` or bundled).
2. Agent generates elu.toml from a natural-language spec, validated
   against the schema on-the-fly.
3. Agent runs `elu check --json` and reads structured errors.
4. Agent fixes specific fields based on error codes, iterates.
5. Agent runs `elu build --json` — success returns the manifest hash.
6. On upgrade / PR / lockfile bump, agent runs `elu explain --json`
   on the new package to render a PR description and compute the
   capability diff against the previously-approved version.
```

The agent never has to memorize the elu.toml schema or the
build pipeline. It queries the schema, iterates on structured
feedback, and consumes structured output. Every step has a
machine-readable form; none requires parsing prose.

---

## Non-Goals

**No build steps.** Not in v1, not on the roadmap. See [What elu
Is Not: A Build System](#what-elu-is-not-a-build-system).

**No "pack the whole directory" escape.** `include` is opt-in.
`include = ["**"]` is valid but has to be typed explicitly.

**No multi-file project format.** One `elu.toml` per package. No
`elu.d/` directory, no `include` pragma pulling in other files, no
inheritance chains. If an organization wants shared authoring
conventions, they publish a template package and downstream authors
use `elu init --template`.

**No pre-build or post-build hooks in elu.toml.** Pre-build is
your build tool's job. Post-build finalization inside the package
itself is `[[hook.op]]`, which runs at install time, not at build
time. "Run this script after elu build" is handled by whatever
invoked elu in the first place (Makefile, CI config, shell
script).

**No publishing from source.** `elu publish` operates on a
package already in the store. Build first, publish second. Two
commands, two responsibilities.

**No `elu.toml` generation from free-form prose inside elu
itself.** An agent generating an elu.toml is an external concern;
elu provides the schema, the validation, the check command, and
the init templates. The agent lives outside elu.

**No `elu build --watch` auto-publishing.** Watch rebuilds the
local manifest on file change. It does not push to a registry. An
author who wants CI auto-publishing writes a CI step that runs
`elu build && elu publish`, same as every other publish workflow.
