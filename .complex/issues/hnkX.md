## Goal

Implement the missing pieces that unblock the publish → install round-trip, and ship the integration test that proves it. Three vertical slices; the final slice's acceptance test IS the round-trip test.

## Why

`elu publish`, `install`, `add`, `remove`, `lock` are all CLI stubs today (`crates/elu-cli/src/cmd/mod.rs` dispatches them to `stub::run`). For a "publish to local registry → pull back → rebuild identically" integration test to exist, the feature work has to land first. This parent scopes just the minimum — `publish` + `install` — to unblock that test. `add`/`remove`/`lock` are deferred.

## What exists vs what's missing

**Server (implemented)** — `crates/elu-registry/src/server/`:
- `begin_publish` handler at `POST /api/v1/packages/{ns}/{name}/{version}`
- `commit_publish` handler at `POST /api/v1/packages/{ns}/{name}/{version}/commit`
- `LocalBlobBackend` for tests; `SqliteRegistryDb::open_in_memory()` for test DB

**Client (partial)** — `crates/elu-registry/src/client/`:
- `fallback.rs::RegistryClient` has `fetch_package`, `fetch_bytes`, `search`, `from_env_str`
- `verify.rs` has `verify_manifest`, `verify_blob`, `verify_layer`
- **Missing:** `publish.rs` — no begin/upload/commit client yet

**CLI (stubbed)** — `crates/elu-cli/src/cmd/`:
- `publish.rs` — returns `"not yet implemented"`
- `install`/`add`/`remove`/`lock` — all dispatched to `stub::run` in `cmd/mod.rs`
- `search.rs` — **implemented; follow this pattern** for publish/install dispatch

**Spec:** [`docs/prd/registry.md`](../../docs/prd/registry.md) (378 lines; publish section around line 70-170)

## Publish-related types (already defined in `crates/elu-registry/src/types.rs`)

```rust
pub struct PublishRequest {
    pub manifest_blob_id: ManifestHash,
    pub manifest: String,      // base64
    pub layers: Vec<PublishLayerRecord>,
    pub visibility: Option<Visibility>,
}
pub struct PublishResponse {
    pub session_id: String,
    pub upload_urls: Vec<UploadUrl>,  // { blob_id, upload_url }
}
pub struct PublishLayerRecord {
    pub diff_id: DiffId, pub blob_id: BlobId,
    pub size_compressed: u64, pub size_uncompressed: u64,
}
```

## Slice breakdown

1. **Client publish library** — `elu-registry/src/client/publish.rs`. Crate-level integration test against the existing axum router.
2. **CLI publish dispatch** — `elu-cli/src/cmd/publish.rs`. Mirrors `search.rs` pattern.
3. **CLI install dispatch + round-trip test** — `elu-cli/src/cmd/install.rs`, rewire `cmd/mod.rs`. Round-trip is the acceptance test.

Each slice leaves the tree in a working state. Slice 2 is blocked by 1; slice 3 by 2.

## Out of scope (filed separately when needed)

- `add`, `remove`, `lock` CLI dispatch
- Publish signatures (PRD says optional; skip in v1)
- Non-default visibility flows beyond `Public`
- Template fetcher for `init --template` (uses registry but orthogonal)

## Risks / step-0 investigation notes

1. **Publisher auth extractor.** Server handlers take a `Publisher` extractor — before writing the client, the first step is to read `crates/elu-registry/src/server/publish.rs` + existing `tests/server_integration.rs` to see how tests bypass or mock auth. Slice 1's red starts there.
2. **Blob gathering from store.** Client needs to enumerate a package's layer blobs by `BlobId`. Build writes them; stack reads them. If the exposed store API doesn't cover this enumeration directly, a small helper lands in `elu-store` (note in slice 1 when it surfaces).
3. **Scope creep.** `add`/`remove`/`lock` are tempting but unrelated to the round-trip. Do not pull them in here.
