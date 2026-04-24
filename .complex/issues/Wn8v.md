## Scope (vertical slice 1 of 3)

Add the missing client-side publish protocol. One public entry point; internal `begin → upload → commit` sequencing. Crate-level integration test proves it drives the real server end-to-end.

## Files

- `crates/elu-registry/src/client/publish.rs` (new)
- `crates/elu-registry/src/client/mod.rs` (add `pub mod publish;`)
- `crates/elu-registry/tests/client_publish.rs` (new)

## Target public API (sketch — refine during red)

```rust
// crates/elu-registry/src/client/publish.rs
pub async fn publish_package(
    client: &RegistryClient,
    store: &Store,                // or whatever the store handle is
    pkg_ref: &PackageRef,         // ns/name@version
    visibility: Visibility,
) -> Result<PackageRecord, RegistryError>;
```

Internally:
1. Load manifest bytes from store by `pkg_ref`; compute `ManifestHash`.
2. Enumerate layer records `[(DiffId, BlobId, size_compressed, size_uncompressed)]` by reading the manifest.
3. `POST /api/v1/packages/{ns}/{name}/{version}` with `PublishRequest` → receive `PublishResponse { session_id, upload_urls }`.
4. For each `UploadUrl`: read the blob bytes from store by `BlobId`, `PUT` to `upload_url`.
5. `POST /api/v1/packages/{ns}/{name}/{version}/commit` → receive `PackageRecord`.

## Step 0 — auth pattern (do BEFORE writing red)

Read `crates/elu-registry/src/server/publish.rs` and `crates/elu-registry/tests/server_integration.rs`. Confirm:
- How the `Publisher` extractor is populated (auth middleware? test header?)
- How existing server-integration tests build a request that satisfies it
- Whether the test can inject a static `Publisher` via a test-only middleware layer

Record the finding as a comment on this cx node before proceeding. If the pattern is ugly enough that the client would need test-specific auth plumbing, flag it — we may need a small server refactor first.

## Test — `tests/client_publish.rs`

Pattern from `tests/server_integration.rs`:
- Build axum router with `LocalBlobBackend` + `SqliteRegistryDb::open_in_memory()`
- Bind to `127.0.0.1:0`; spawn server task
- Construct a fixture manifest + layer blobs; seed them into a store
- Run `publish_package(...)` against the server
- Assert:
  - Returned `PackageRecord` matches input (ns, name, version, blob IDs)
  - Server DB `get_version(ns, name, version)` returns the same record
  - Blob backend contains each blob at its expected key

## Red/green

- **Red:** skeleton `publish_package` with `todo!()`; test written in full; runs and panics at `todo!()`. Commit `red — …`.
- **Green:** implement the three HTTP calls. Use `reqwest` (already a registry dep) with the existing `RegistryClient`'s HTTP client if exposed, or a fresh one. Commit `green — …`.

## Out of scope

- Publish signatures (optional per PRD).
- Rollback on partial failure (the server already rejects uncommitted sessions; client surfaces errors but doesn't need its own cleanup protocol).
- CLI wiring (slice 2).

## Known unknown

Blob gathering from store — if the store API doesn't directly support "give me the bytes for BlobId X from package Y", a small helper may need to land in `elu-store` as part of this slice. Not a blocker; note it when it surfaces.
