## Scope

Make `crates/elu-registry/src/blob_store.rs::LocalBlobBackend` a real working blob backend: persist bytes, serve PUT (with hash verification), serve GET. Round-trip test then drops its inline `InMemoryBlobBackend` and uses the real one.

## Current state (89-line file)

`LocalBlobBackend` only generates URLs (`upload_url` / `download_url` that point at `<base_url>/blobs/<blob_id>`) and tracks an "uploaded" `HashSet<String>`. There is no HTTP listener and no byte storage anywhere — the server's begin/commit handlers return URLs that point at nothing.

Existing tests work around this:
- `crates/elu-registry/tests/client_publish.rs` stands up its own axum `PUT /blobs/{blob_id}` handler that hashes the body and calls `LocalBlobBackend::mark_uploaded`. No GET (slice 1's test never fetched).
- `crates/elu-cli/tests/roundtrip.rs` builds an inline `InMemoryBlobBackend` (`Arc<Mutex<HashMap<BlobId, Vec<u8>>>>`) and serves both PUT and GET on a separate axum listener. This shim is what should go away after this slice.

## Target shape

In-memory storage is sufficient (PRD says LocalBlobBackend is dev/test/self-host tier; real deployments use S3/GCS via presigned URLs). Either:

**Option A — backend exposes a mountable router (preferred):**
```rust
impl LocalBlobBackend {
    pub fn router(self: Arc<Self>) -> axum::Router { ... }
    // store: Mutex<HashMap<BlobId, Vec<u8>>>
}
```
Caller (real registry server, tests) constructs the backend, takes its router, and either mounts it under the same listener as the registry API or serves it on its own port. Keeps the backend self-contained.

**Option B — backend bundles its own listener:**
```rust
impl LocalBlobBackend {
    pub async fn spawn(addr: SocketAddr) -> Result<(Self, JoinHandle<...>)> { ... }
}
```
Simpler caller code, but the listener-lifecycle in tests is awkward.

A is cleaner; pick A unless a strong reason emerges in step 0.

## PUT semantics

- Path: `PUT /blobs/{blob_id}` — `{blob_id}` parses via `BlobId::from_str`
- Hash the body with `elu_store::hasher::Hasher`; if `BlobId(actual) != blob_id` → 400
- Otherwise persist bytes and call `mark_uploaded`. 200 OK.

## GET semantics

- Path: `GET /blobs/{blob_id}` — return bytes if present, 404 otherwise
- No auth (PRD: dev tier; real backends use presigned URLs)

## Test expectations

After this slice:
- `crates/elu-registry/src/blob_store.rs` gains a small set of unit tests for the router (PUT + GET roundtrip; PUT-with-wrong-hash → 400; GET-not-present → 404)
- `crates/elu-registry/tests/client_publish.rs` simplifies — drop the inline PUT handler, mount the backend's router into an axum listener
- `crates/elu-cli/tests/roundtrip.rs` simplifies — drop `InMemoryBlobBackend`, use `LocalBlobBackend` with its router

The full `cargo test -p elu-registry` and `cargo test -p elu-cli` suites must still pass.

## Step 0 (before red)

Read `crates/elu-registry/src/server/mod.rs` (or wherever `router(state)` is built — `client_publish.rs` calls `elu_registry::server::router`) to see how the server's axum router is composed. Decide whether the blob router gets mounted into the same server router or runs on its own listener. Note the choice in a cx comment before writing red.

## Files

- `crates/elu-registry/src/blob_store.rs` — add storage + router; keep the existing `BlobBackend` trait API
- `crates/elu-registry/tests/client_publish.rs` — simplify (no separate PUT handler)
- `crates/elu-cli/tests/roundtrip.rs` — drop `InMemoryBlobBackend`, use `LocalBlobBackend`
- Possibly `crates/elu-registry/src/server/mod.rs` if mounting choice goes that way

## Red/green

Red: failing test (or new unit tests in blob_store.rs) for PUT-then-GET round trip. Existing tests stay as-is for the moment so nothing breaks compilation. Commit `red — …`.

Green: implement storage + router; simplify the two callers. All tests pass. Commit `green — …`.

## Out of scope

- Disk-backed storage (in-memory is fine for dev tier)
- Auth / signed URLs
- Quotas, eviction
