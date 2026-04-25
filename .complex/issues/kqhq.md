## Why

`MqEx` (multi-ref install + transitive registry resolution) needs the registry-backed `VersionSource` to implement `fetch_by_hash`. The HTTP API today is keyed by `(ns, name, version)` only, so a fresh-store install whose lockfile pins deps to manifest hashes has no way to fetch them. Workarounds (search cached records, fall through to error) don't survive contact with a clean clone + lockfile.

## Scope

Add `GET /api/v1/manifests/:hash` returning the same `PackageRecord` shape as the named lookup. Same response = client gets manifest_url + per-layer urls in one round-trip; `RegistrySource` gets a real `fetch_by_hash`.

## Files

- `crates/elu-registry/src/error.rs` — new `RegistryError::ManifestHashNotFound { hash }` variant.
- `crates/elu-registry/src/db/sqlite.rs` — `get_version_by_manifest_hash` + `_with_visibility` (lookup `(ns,name,version)` keyed by `manifest_blob_id`, delegate to existing `get_version`).
- `crates/elu-registry/src/server/fetch.rs` — handler + route `/api/v1/manifests/{hash}`, OptionalPublisher auth.
- `crates/elu-registry/src/server/error.rs` — map `ManifestHashNotFound` → 404.
- `crates/elu-registry/src/client/fallback.rs` — `RegistryClient::fetch_package_by_hash(hash)`.
- Tests: db unit tests + a server integration test (publish → fetch by hash → assert == named lookup; missing → 404; private without auth → 404).

## Acceptance

- DB lookup returns the same `PackageRecord` as `get_version(ns, name, version)` for a known package; returns `ManifestHashNotFound` for unknown hash.
- `GET /api/v1/manifests/<hash>` returns the JSON record for a public package; 404 for unknown hash; 404 for a private package without matching auth.
- `RegistryClient::fetch_package_by_hash` round-trips against an in-process registry.
- Visibility behavior matches `get_version_with_visibility` (don't leak existence of private packages).

## Out of scope

- A `GET /api/v1/manifests/:hash/bytes` for raw manifest bytes — caller still uses `fetch_bytes(record.manifest_url)`.
- Any client-side caching across processes.

## Blocks

`MqEx` (reset its blocked-by to include this slice).
