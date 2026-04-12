# CLI

`elu` is the single command users and scripts invoke to interact with
every other component. It is a thin wrapper over the store, resolver,
stackers, importers, outputs, and registry — the CLI never implements
logic of its own, it dispatches to the component whose behavior the
user is asking for.

This document describes the command surface, not the argument parsing
library. The shape is designed to read naturally: `elu <verb>
<object>`, with verbs that correspond to concrete operations in the
ring model (see [README.md](README.md)).

---

## Global Flags

| Flag | Effect |
|------|--------|
| `--store <path>` | Override the store root. Default: `$ELU_STORE` or `~/.local/share/elu`. |
| `--registry <url>` | Override the registry. Comma-separated list for fallback chain. |
| `--offline` | Never contact a registry. Fail if resolution needs one. |
| `--locked` | Refuse to proceed if the lockfile would need to change. |
| `--json` | Machine-readable output on stdout. |
| `-v`, `-vv` | Verbose logging. |
| `-q` | Quiet: suppress progress output. |
| `--help` | Print help for the command. |

Global flags can appear before or after the subcommand.

---

## Verbs

### `elu install <ref>...`

Resolve and stack the referenced packages into the current project's
default target (`./elu-out/` by default, overridable with `-o`).

```
elu install ox-community/postgres-query
elu install ox-community/postgres-query ox-community/shell
elu install -o /srv/skills ox-community/postgres-query@^0.3
```

Writes or updates the lockfile (`elu.lock` in the current directory).
With `--locked`, fails if resolution would change the lockfile.

### `elu add <ref>...`

Add a reference to the current project's manifest without immediately
stacking. Updates both `manifest.toml` (or equivalent) and `elu.lock`.

```
elu add ox-community/postgres-query@^0.3
```

### `elu remove <name>`

Remove a reference from the project manifest. Updates the lockfile.

### `elu lock`

Resolve the project manifest and write `elu.lock`. Does not stack
anything.

```
elu lock             # regenerate the lockfile
elu lock --locked    # error if the lockfile would change (CI use)
```

### `elu update [<name>...]`

Re-resolve ignoring the lockfile's pins, then overwrite the
lockfile. With specific names, update only those packages and their
transitive deps.

```
elu update
elu update ox-community/postgres-query
```

### `elu stack <ref>... -o <path>`

Resolve, fetch any missing blobs, and materialize the result at
`<path>`. The format is inferred from the path or set explicitly.

```
elu stack ox-community/postgres-query -o ./out
elu stack ox-community/postgres-query -o skill.tar.zst
elu stack ox-runner-image -o runner.qcow2 --base debian/bookworm-minbase
```

See [outputs.md](outputs.md) for format-specific options.

### `elu build <manifest>`

Take a local manifest file, hash and store any referenced layer
files, and put the manifest in the store. This is how hand-authored
packages enter the store without going through an importer.

```
elu build ./manifest.toml
```

`elu build` does **not** publish. Publishing is a separate step.

### `elu publish <ref>`

Push a package already present in the local store to the registry.

```
elu publish ox-community/postgres-query@0.3.0
```

Requires authentication. See [registry.md](registry.md) for the
publish protocol.

### `elu import <type> <name>...`

Run an importer. The `<type>` is one of the built-in importers.

```
elu import apt curl
elu import apt --closure curl jq
elu import npm express --closure
elu import pip requests==2.31.0
```

Imported packages land in the local store under the importer's
reserved namespace (`debian`, `npm`, `pip`). They can be used as
dependencies of hand-authored packages immediately; publishing them
to a registry is a separate `elu publish`.

### `elu search <query>`

Query the registry's search index.

```
elu search postgres
elu search --kind ox-skill database
elu search --tag review
elu search --namespace ox-community
```

### `elu inspect <ref>`

Show a package's manifest, resolved dependencies, and layer list.

```
elu inspect ox-community/postgres-query@0.3.0
elu inspect b3:8f7a...
elu inspect --json ox-community/postgres-query
```

### `elu ls`

List packages present in the local store.

```
elu ls                          # all refs
elu ls ox-community             # namespace filter
elu ls --kind ox-skill          # kind filter
```

### `elu gc`

Run garbage collection on the store. See [store.md](store.md).

```
elu gc
elu gc --dry-run                # report what would be freed
```

### `elu fsck`

Re-hash every object in the store and report mismatches.

```
elu fsck
elu fsck --repair               # delete bad objects (they will be re-fetched)
```

### `elu refs`

Low-level ref operations.

```
elu refs ls
elu refs set <ns>/<name>/<version> <hash>
elu refs rm  <ns>/<name>/<version>
```

Used by tooling and debugging. Most users never touch this directly.

### `elu config`

Print or edit the user's elu configuration.

```
elu config show
elu config set registry https://registry.elu.dev
elu config set store ~/elu-store
```

Config is stored in `$XDG_CONFIG_HOME/elu/config.toml`.

---

## Project Files

An elu project is a directory containing:

```
my-project/
  manifest.toml       # the package being authored, or a consumer project
  elu.lock            # the pinned resolution, committed to VCS
  layers/             # optional: source trees to be packed into layers
  .elu/               # optional: per-project cache and overrides
```

A **consumer project** is one that declares dependencies but does not
itself get published. Its `manifest.toml` has a minimal `[package]`
block and a `[[dependency]]` list; its purpose is to drive `elu
install` and `elu lock`.

An **authored package project** is one whose `manifest.toml` describes
a package that will be published. `elu build` turns the local
`layers/` contents into layer blobs and stores the manifest; `elu
publish` pushes the result.

The CLI auto-detects the mode from the manifest shape: a manifest
with `[[layer]]` entries is an authored package; one without is a
consumer project.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Generic failure |
| 2 | Usage error (bad flags, missing argument) |
| 3 | Resolution failure (no version satisfies, conflict) |
| 4 | Network failure (registry unreachable, blob fetch failed) |
| 5 | Store failure (disk full, permission denied) |
| 6 | Hook failure (post-unpack hook returned non-zero) |
| 7 | Lockfile mismatch (`--locked` with changes needed) |

Scripts should branch on these; output on stderr explains which
specific cause produced the code.

---

## Output Conventions

Default human output is colored, multi-line, progress-bar where
appropriate. `--json` output is single-line-per-event on stdout for
streaming operations (`install`, `stack`, `import`) and a single JSON
object for query operations (`inspect`, `ls`, `search`).

Errors always go to stderr. Progress always goes to stderr. Machine-
readable output on `--json` always goes to stdout. Piping `elu --json
install` through `jq` works without special handling.

---

## Shell Completion

`elu completion <shell>` emits completion scripts for bash, zsh, and
fish. Completions cover subcommand names, package references from
the local store, and flag values where a fixed set exists (formats,
compression types, kinds).

---

## Non-Goals

**No interactive TUI.** elu is a scripting-friendly CLI. A TUI can be
built on top by a separate tool consuming `--json` output.

**No daemon mode.** Every `elu` invocation is a fresh process
operating directly on the store. A long-running daemon would add
state we do not need and create lifecycle bugs we do not want.

**No language-specific wrappers in the main distribution.** Other
tools that want to invoke elu programmatically use the CLI with
`--json` output or link against a future library crate. The CLI is
the public API surface for v1.
