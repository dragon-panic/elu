# Output formats (dir, tar, qcow2)

Materialize a resolved stack into a concrete artifact. Every output
implements the same contract: take a finalized staging directory,
produce a target, clean up.

**Spec:** [`docs/prd/outputs.md`](../docs/prd/outputs.md)

## Key decisions (from PRD)

- Contract: `materialize(staging_dir, target_path, options) → result`.
  By the time an output is called, layers are applied and the hook
  has run. Outputs never resolve, never stack, never mutate the
  store.
- `dir`: rename staging into place atomically (same-fs) or copy.
  `--force` to replace an existing target.
- `tar`: stream staging as tar, sorted paths for byte-reproducibility,
  optional streaming compression (`gzip`, `zstd`, `xz`). Compression
  is transport, not content.
- `qcow2`: requires an `os-base` package declared via `--base`. Base
  image's `metadata.os-base.finalize` runs inside the guest — the
  only place elu executes guest-side code. Declared, not inferred.
- Format inferred from target path suffix; `--format` always wins.
- Extension set is closed in v1. No plugin boundary. Adding formats
  is an elu code change.

## Acceptance

- Three formats work: `dir`, `tar`, `qcow2`.
- `tar` output is byte-reproducible given the same resolution.
- `qcow2` boots on QEMU given a valid `os-base` package.
- `--format` and path inference agree on which format to use.
- Partial outputs are never committed (failure = nothing at target).
