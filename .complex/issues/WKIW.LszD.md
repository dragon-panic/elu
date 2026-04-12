# Package manifest format

The structured document that describes a package: namespace, name,
version, kind, description, tags, ordered layer list, dependencies,
optional post-unpack hook, free-form consumer metadata.

**Spec:** [`docs/prd/manifest.md`](../docs/prd/manifest.md)

## Key decisions (from PRD)

- TOML on disk, equivalent JSON on the wire.
- `schema = 1`. Unknown fields preserved but ignored by elu itself.
- `kind` is opaque to elu — `native` is the default, everything else
  is interpreted by consumers. See consumers.md.
- `tags` are free-form discovery strings, never load-bearing.
- `[hook]` is per-package (not per-layer), host-side, argv list, 60s
  default timeout, runs in the staging directory with `ELU_STAGING`
  set.
- `[metadata]` is a free-form table elu preserves verbatim — this is
  where consumer-specific fields (like `metadata.ox.requires`) live.

## Acceptance

- Parse and serialize manifests.
- Validate: schema version, namespace/name charset, semver version,
  non-empty kind, well-formed layer hashes, semver-or-hash dependency
  constraints, non-empty hook command.
- Reject manifests whose referenced layer blobs cannot be made
  present in the store or fetch plan.
- Canonical manifest bytes produce a stable hash (the package's
  identity).
