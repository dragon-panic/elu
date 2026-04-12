# Content-addressed store

The object database that underlies everything else. Holds manifests
and layer blobs addressed by hash. Filesystem-backed, atomic writes,
lock-free reads, exclusive-lock GC.

**Spec:** [`docs/prd/store.md`](../docs/prd/store.md)

## Key decisions (from PRD)

- Addresses are `<algo>:<hex>`. v1 picks a single algorithm
  (BLAKE3 or SHA-256) but the prefix is part of the canonical
  identity so migration is possible later.
- Layout: `objects/<algo>/<2-hex>/<rest>`, `refs/<ns>/<name>/<version>`
  as one-line files, `tmp/` for staging.
- Writes: stream to `tmp/`, hash on the fly, rename on commit.
- Reads: direct path lookup, no verification on the hot path.
- Refs are append-only; re-publishing a version is rejected.
- GC is mark-and-sweep, manual (`elu gc`), takes exclusive lock.
- No reverse index, no database, no encryption, no distribution.

## Acceptance

- `put(bytes) → hash` deduplicates when the object already exists.
- `get(hash) → bytes | not_found` is lock-free.
- `put_manifest` validates before storing.
- Ref writes are atomic.
- `gc()` reclaims every object not transitively reachable from any
  ref.
- `fsck()` re-hashes every object and reports mismatches.
- Two concurrent writers of identical bytes converge safely.
