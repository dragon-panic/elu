# Registry (publish/fetch)

Thin HTTP lookup service that maps `namespace/name@version` to a
manifest hash and tells clients where to fetch the blobs. Not a
blob host.

**Spec:** [`docs/prd/registry.md`](../docs/prd/registry.md)

## Key decisions (from PRD)

- Registry stores metadata (hash, kind, description, tags, publisher,
  blob URLs, optional signature) — **never** the manifest or blob
  bytes. Bytes live in operator-chosen object storage reached via
  presigned URLs.
- Publish flow: `POST /packages/<ns>/<name>/<version>` with manifest
  + blob list → server returns upload URLs for missing blobs →
  client PUTs blobs → `POST /commit` finalizes. Atomic; versions
  immutable once committed.
- Fetch flow: `GET /packages/<ns>/<name>/<version>` returns the
  manifest hash and blob URLs; client fetches via plain HTTP and
  verifies every byte by hash. A compromised registry cannot
  substitute content.
- Semver resolution is client-side. Registry just lists versions.
- Namespaces are publisher-scoped. Verified publishers get a badge.
  Reserved: `debian/`, `npm/`, `pip/` (not directly publishable).
- Visibility: public or org-private. Public packages cannot depend
  on private ones (rejected at publish).
- `$ELU_REGISTRY` supports a fallback chain (comma-separated).
- Self-hostable: the HTTP API is the contract, implementation is not.

## Acceptance

- Publish API: begin → upload → commit, atomic visibility.
- Fetch API: package record with blob URLs, version list endpoint.
- Search API: `q`, `kind`, `tag`, `namespace` filters.
- Client verifies every fetched byte against declared hashes.
- Private package visibility is enforced on read as well as publish.
- Registry reachable via `ELU_REGISTRY` chain with fallback.
