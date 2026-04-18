# elu-author: Authoring Workflow Implementation

Counterpart to [../prd/authoring.md](../prd/authoring.md). This document
records the implementation choices in `crates/elu-author`. The PRD is
the contract; this doc is how we satisfy it.

## Crate shape

`crates/elu-author` is a **library crate**. The CLI binary that wires
it to `clap` subcommands (`elu build`, `elu init`, `elu check`, `elu
explain`, `elu schema`) lives in the CLI task (`jfvm`); every author-
side verb in the PRD maps to a public function here.

Dependencies: `elu-manifest`, `elu-store`, `elu-registry` (for template
fetch). `notify` v6 drives watch mode. `tar`, `globset`, `walkdir`,
`sha2` are standard.

### Module layout

| Module | Responsibility |
|---|---|
| `lib.rs` | Re-exports the public surface: `build`, `check`, `init`, `explain`, `schema`, `report`, `watch`. |
| `build.rs` | The pipeline: parse → validate → walk → tar → `put_blob` → stored manifest → `put_manifest` + `put_ref`. |
| `walk.rs` | Resolve `include`/`exclude`/`strip`/`place` against the project root. Rejects absolute paths and `..`. Sorted output. |
| `sensitive.rs` | Globset for `.env*`, `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*`, `.ssh/**`, `.netrc`, `.aws/credentials`, `.git/**`. |
| `tar_det.rs` | Deterministic tar: sorted, uid/gid 0, mtime 0, mode from fs or layer default, via `tar::HeaderMode::Deterministic`. |
| `check.rs` | Same front half as `build` minus packing: surfaces every diagnostic as a `Report`. |
| `init.rs` | Built-in templates via `include_str!`; `init_from_template` fetches and unpacks a registry template package with a `TemplateProvider` trait. |
| `infer.rs` | `--from` heuristics: detect `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `Makefile`, docs; emit TOML with TODOs. |
| `explain.rs` | Plain-English `explain_text` over a `Manifest`; `diff_manifests` reports version, dep adds/removes, hook-op adds/removes. |
| `schema.rs` | Hand-written JSON Schema for source and stored forms. |
| `report.rs` | Stable `ErrorCode` enum + `Report` + `Diagnostic` shape. This is the type that crosses `--json`. |
| `watch.rs` | Per-layer `(path, size, mtime)` fingerprint hash; `incremental_build` repacks only layers whose fingerprint changed. |

## Design decisions

1. **Library, not binary.** `build()`, `check()`, `init_builtin()`,
   `init_from_template()`, `explain_text()`, `diff_manifests()`,
   `source_schema()`, `stored_schema()`, `incremental_build()` all
   return `Report` or a structured value. The CLI renders prose or
   JSON from there.
2. **Deterministic tar is a fresh implementation.** `elu-import`'s
   `tar_layer::build_tar` was left alone — retrofitting its
   `append_dir_all` to produce sorted, zero-uid/gid/mtime output
   would churn every importer's `diff_id`. `tar_det.rs` is purpose-
   built for the author path.
3. **Strict build/package boundary.** `build.rs` never shells out,
   never runs user code. Hook ops are validated as data; their
   execution is `elu-hooks`' job at install time.
4. **Structured errors first.** `report.rs` is the type the whole
   crate produces. Human rendering is future work in the CLI task;
   the library returns `Report`.
5. **Incremental watch keyed on fingerprint.** `(path, size, mtime)`
   sorted + SHA-256. No content hash needed — the author is editing
   files, and the mtime skew is dwarfed by the repack time.
6. **Templates embedded.** Built-in templates live under
   `src/templates/*.toml` and compile in via `include_str!`.
7. **Registry template unpacking.** Handled locally with a
   `TemplateProvider` trait rather than depending on the yet-unwritten
   layer-stacker crate (`zRCQ`). We fetch the manifest, then each
   layer blob, verify `diff_id`, and extract tar entries into the
   target directory. Paths starting with `/` or containing `..` are
   rejected.
8. **JSON Schema is hand-written.** Small and stable; worth avoiding
   a schemars dep plus its overrides. Drift is caught by a
   round-trip test against the worked examples from the PRD.

## Error codes

Stable kebab-case identifiers. Agents dispatch on these; the human
prose in `message` and `hint` can evolve. Source of truth is
`report.rs::ErrorCode`.

- `schema-unsupported`
- `package-namespace-invalid`
- `package-name-invalid`
- `package-kind-invalid`
- `package-description-invalid`
- `package-version-invalid`
- `layer-missing-include`
- `layer-mixed-form`
- `layer-include-no-matches`
- `layer-absolute-path`
- `layer-parent-escape`
- `glob-invalid`
- `dep-ref-invalid`
- `dep-version-invalid`
- `hook-op-unknown-type`
- `hook-op-bad-path`
- `hook-op-path-not-produced` — **warning only in v1.** Agents
  that need strict cross-package validation must wait for the
  resolver.
- `sensitive-pattern` — warning by default; promoted to an error
  under `--strict`.
- `file-not-readable`
- `store-error`
- `toml-parse`

## Known v1 deferrals

- **Cross-package hook-op path validation.** A chmod/delete path
  that targets a dependency layer cannot be validated without a
  resolver. We emit `hook-op-path-not-produced` as a warning and
  carry the gap in the error-code reservation.
- **Lockfile.** The PRD mentions `elu.lock` and `--locked` under
  the build pipeline; the resolver (`wX0h`) is not yet built. This
  crate does not write a lockfile. When the resolver lands, it
  will own the lockfile; `build.rs`'s pipeline has a clean seam
  for it.
- **`elu schema --errors`.** Emitting the code list as JSON for
  programmatic consumption is not implemented. The code list in
  this doc and in `report.rs` is the contract.
- **Interactive `elu init`.** The CLI task (`jfvm`) owns prompts;
  this crate exposes only the non-interactive paths.
- **Hook-op `run`.** Out of v1 per the hooks design; the author
  crate does not special-case it.

## Test strategy

Integration tests live in `crates/elu-author/tests/`. Each slice has
one red/green test file driving one capability end-to-end:

- `walk.rs` — walk + strip/place + globset rejection rules
- `sensitive.rs` — sensitive pattern matcher
- `tar_det.rs` — determinism across temp dirs, sort, zeroed headers
- `build.rs` — end-to-end `build()` producing a manifest in the CAS
- `build_check.rs` — `--check` side-effect-free
- `build_strict.rs` — warning promotion
- `init_builtin.rs` — each built-in template parses + validates
- `init_from.rs` — inference across Cargo/npm/pyproject/go.mod
- `init_template.rs` — provider fetches manifest + blobs, verifies
  diff_id, unpacks tar
- `check.rs` — `Report` shape and stable JSON
- `explain.rs` — text rendering and `--diff`
- `schema.rs` — source/stored schema structural invariants + PRD
  example round-trip
- `watch.rs` — per-layer fingerprint drives incremental repack
