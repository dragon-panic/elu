# CLI (clap)

The `elu` command surface. Thin dispatch layer over the store,
resolver, stacker, importers, outputs, and registry. No logic of its
own — it translates arguments into component calls.

**Spec:** [`docs/prd/cli.md`](../docs/prd/cli.md)

## Key decisions (from PRD)

- Shape: `elu <verb> <object>`. Verbs map to ring-model operations.
- Global flags: `--store`, `--registry`, `--offline`, `--locked`,
  `--json`, `-v`/`-vv`, `-q`.
- Verbs:
  - Project: `install`, `add`, `remove`, `lock`, `update`
  - Stacking: `stack -o <path> [--format ...]`
  - Authoring: `build`, `publish`
  - Importers: `import apt|npm|pip [--closure]`
  - Discovery: `search`, `inspect`, `ls`
  - Maintenance: `gc`, `fsck`, `refs`, `config`, `completion`
- Project files: `manifest.toml` + `elu.lock` + optional `layers/`.
  Consumer vs authored project detected from manifest shape
  (presence of `[[layer]]`).
- Exit codes: 0 ok, 2 usage, 3 resolution, 4 network, 5 store,
  6 hook, 7 lockfile drift.
- `--json` streams newline-delimited events for long ops, single
  object for queries. Errors and progress to stderr; `--json` to
  stdout.
- No daemon, no TUI, no language wrappers in v1.

## Acceptance

- All verbs present with documented flags.
- `--json` output is stable and documented.
- Exit codes match the table.
- Shell completions for bash, zsh, fish.
- Pipes cleanly: `elu --json install | jq` works without tweaks.
