# Authoring workflow

The author-facing surface: how a human or an agent gets from "I have
some files" to "I have a working elu package." The first-impression
lever for adoption. Scope: the `elu.toml` source format, the build
pipeline, scaffolding and validation commands, and the agent-
friendly properties (JSON schema, structured errors, templates).

**Spec:** [`docs/prd/authoring.md`](../docs/prd/authoring.md)

## Key decisions (from PRD)

- **Strict build/package separation.** elu does not build software.
  It packages files that already exist. Your Makefile (or cargo,
  npm, go, whatever) builds; elu packages. Two tools, two
  responsibilities. Docker's conflation is the anti-pattern.
- **One file at project root.** `elu.toml`. Committed to VCS.
  Source form of the same schema as the stored manifest, with
  build directives on `[[layer]]` entries (`include`, `exclude`,
  `strip`, `place`, `mode`) instead of resolved `diff_id`s.
- **`include` is opt-in.** No "pack everything" default.
  Sensitive-pattern files (`.env`, `*.pem`, `id_rsa*`, `.git/`)
  produce warnings on build; `--strict` promotes to errors.
- **Deterministic build.** Sorted tar entries, uid/gid 0, mtime 0,
  deterministic compression. Same inputs + same elu version =
  same manifest hash byte-for-byte.
- **`elu init` with templates.** Built-in templates per kind
  (`native`, `ox-skill`, `ox-persona`, `ox-runtime`). Registry
  templates are themselves elu packages of `kind = "elu-template"`.
  `--from <dir>` infers a starter from an existing project by
  looking at `Cargo.toml`/`package.json`/`pyproject.toml`/etc.
- **`elu check` for fast validation.** Parses, validates schema,
  resolves deps, checks include patterns match at least one file,
  pre-checks hook ops. Does not build layer blobs.
- **`elu explain <ref>`.** Plain-English package summary for human
  review and agent-generated PR descriptions. `--diff` form for
  capability diff between two versions.
- **`elu schema`.** Emits JSON Schema for offline validation by
  agents that don't want elu in the path.
- **Structured errors with stable codes.** Every command supports
  `--json`. On failure, output is `{ok: false, errors: [{field,
  code, message, hint, file, line}], warnings: [...]}`. Codes are
  documented and stable across minor versions; agents dispatch on
  `code`, humans read `message` + `hint`.
- **Source vs stored form validation.** A `[[layer]]` entry must
  have either source-form fields (with `include` required) or
  stored-form fields (with `diff_id` required). Mixing is
  rejected. `elu build` consumes source, emits stored.

## Acceptance

- Parse `elu.toml` source form (with `include`/`exclude`/`strip`/
  `place`/`mode` on layer entries).
- `elu build` walks include patterns, produces deterministic tar
  blobs, writes stored-form manifest to the CAS, returns manifest
  hash.
- `elu build --check` validates without building.
- `elu build --watch` rebuilds on file changes, incrementally
  (only layers whose files changed are repacked).
- `elu build --strict` fails on sensitive-pattern warnings.
- `elu init` with built-in templates for `native`, `ox-skill`,
  `ox-persona`, `ox-runtime`.
- `elu init --from <dir>` infers a starter with TODO comments.
- `elu init --template <ref>` fetches and instantiates a registry
  template.
- `elu check` validates and reports structured errors.
- `elu explain` renders plain-English and `--json` summaries,
  including `--diff` capability diff.
- `elu schema` emits JSON Schema document for source and stored
  forms.
- All author-side commands support `--json` with the stable error
  schema.
- Sensitive-pattern warning logic covers `.env*`, `*.pem`, `*.key`,
  `id_rsa*`, `id_ed25519*`, `.ssh/*`, `.netrc`, `.git/**`.

## Dependencies

Blocked on:
- `LszD` (manifest format) — authoring is the source form of the
  same schema.
- `VJp1` (store) — `elu build` writes to the store.

Blocks:
- `jfvm` (CLI) — needs `init`, `build`, `check`, `explain`,
  `schema` as first-class verbs.
