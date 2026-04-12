# Importers (apt, npm, pip)

Bridge external package ecosystems into elu. Each importer takes an
external reference and produces native elu packages — same shape as
hand-authored ones, same resolver, same stacker, same outputs.

**Spec:** [`docs/prd/importers.md`](../docs/prd/importers.md)

## Key decisions (from PRD)

- 1:1 mapping: one upstream package → one elu package with one
  layer. Upstream dependency edges become elu `[[dependency]]`
  edges. Preserves dedup and provenance.
- Reserved namespaces: `debian/`, `npm/`, `pip/`. Imported packages
  carry `kind = "debian" | "npm" | "pip"` and their original
  control/metadata in `[metadata.<type>]`.
- `--closure` walks transitive upstream deps and imports them all.
  Without it, deps are left as unresolved elu references.
- Upstream tarballs are cached alongside the store and subject to the
  same GC rules.
- postinst/preinst/lifecycle scripts are **not** executed. Imported
  layers are just files; lifecycle is the consumer's concern.
- No source builds. `.deb`, tarball, wheel only.
- v1 scope: runtime `dependencies` only for npm (no dev/optional/peer).
  Wheels only for pip (no sdist).

## Acceptance

- `elu import apt <name>` produces a valid `debian/<name>` package.
- `elu import npm <name>` produces a valid `npm/<name>` package.
- `elu import pip <name>` produces a valid `pip/<name>` package.
- `--closure` resolves and imports transitive deps.
- Imported packages compose with hand-authored ones through the
  normal resolver (no special-case code paths).
