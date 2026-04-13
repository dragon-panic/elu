# Registry

The registry is the service that lets publishers share elu packages
across machines, teams, and organizations. It is the component that
turns "a directory of hashed objects on my laptop" into "a package
ecosystem."

The registry is deliberately thin. It is a **lookup service**, not a
package host. It maps `namespace/name@version` to a manifest hash and
tells clients where to fetch the blobs. The source of truth for bytes
is always the content-addressed store — on the client side locally,
and in whatever blob storage the registry operator happens to use on
the server side.

This thinness is intentional. A fat registry that stores, signs,
scans, and re-serves every blob is a different project with different
trust and operational concerns. elu's registry does the minimum needed
to make names portable across stores.

---

## What the Registry Stores

For each published package version:

| Field | Meaning |
|-------|---------|
| `namespace/name` | The reference. Unique across the registry. |
| `version` | Semver string. Unique within a `namespace/name`. |
| `manifest_blob_id` | Hash of the manifest bytes. The package's identity. |
| `manifest_url` | Where to fetch the manifest bytes. |
| `kind` | From the manifest. Indexed for search. |
| `description` | From the manifest. Indexed for search. |
| `tags` | From the manifest. Indexed for search. |
| `layers` | Per-layer distribution record: `diff_id`, `blob_id`, `url`, compressed and uncompressed sizes. See below. |
| `publisher` | Verified identity that published this version. |
| `published_at` | Timestamp. |
| `signature` | Optional publisher signature over the manifest blob_id. |

The `layers` array is the bridge from the manifest (which names
layers by `diff_id`) to the actual bytes a client can fetch (keyed
by `blob_id`). Each entry:

```json
{
    "diff_id":           "b3:cc...",   // hash of uncompressed tar; matches the manifest
    "blob_id":           "b3:ff...",   // hash of stored bytes
    "url":               "https://blobs.example/b3/ff/...",
    "size_compressed":   4123,          // bytes on the wire / on disk
    "size_uncompressed": 18432          // bytes after decompression; matches manifest
}
```

A different publisher shipping the same source tree with a
different compressor produces a record with the same `diff_id` but
a different `blob_id` and `url`. Clients that already have a blob
matching this `diff_id` (in any encoding) can skip the fetch; the
diff_id is the shared cache key. Clients that don't fetch from
`url` and verify.

Note what is **not** in this list: the manifest bytes themselves
and the layer bytes themselves. The registry stores only metadata
and pointers. Bytes live in object storage managed by the registry
operator (S3, GCS, a CDN, a plain HTTP server) — elu does not care,
as long as clients can `GET` them.

---

## Publishing

Publishing is atomic: a version is either entirely published or
entirely absent. There is no half-published state visible to other
clients.

```
POST /api/v1/packages/<namespace>/<name>/<version>
Content-Type: application/json
Authorization: Bearer <publisher token>

{
    "manifest_blob_id": "b3:8f7a...",
    "manifest":         "<base64-encoded manifest bytes>",
    "layers": [
        {
            "diff_id":           "b3:cc...",
            "blob_id":           "b3:ff...",
            "size_compressed":   4123,
            "size_uncompressed": 18432
        }
    ]
}
```

The flow:

1. Client `POST`s the manifest bytes and the per-layer distribution
   records. The client has already computed both IDs locally during
   `put_blob`.
2. Server validates the manifest (same rules as `store.put_manifest`)
   and checks that every `diff_id` in the manifest appears in the
   `layers` array.
3. Server checks that `namespace/name@version` is not already taken.
4. Server returns per-layer `upload_url`s for any `blob_id` it does
   not already have in its blob store. For blob_ids already present
   (shared with another package), it returns nothing — the layer is
   already distributable.
5. Client `PUT`s each missing blob to its `upload_url`.
6. Client `POST`s `/commit` to finalize:

```
POST /api/v1/packages/<namespace>/<name>/<version>/commit
```

7. Server verifies all referenced blobs are present in its blob
   store, then writes the registry record. Only now does the version
   become visible to other clients.

Re-publishing the same `namespace/name@version` is rejected with a
hard error. Versions are immutable once committed. A publisher who
needs to fix a release cuts a new version.

### Why upload URLs

Presigned `upload_url`s let clients push blobs directly to object
storage without streaming through the registry API process. This
keeps the registry light and lets operators use any blob backend they
like. An operator who prefers to proxy uploads can return upload URLs
that point at their own API and handle the forwarding.

---

## Fetching

A client resolves `namespace/name@version` and gets back the manifest
blob_id plus the per-layer distribution records.

```
GET /api/v1/packages/<namespace>/<name>/<version>
```

Response:

```json
{
    "namespace":        "ox-community",
    "name":             "postgres-query",
    "version":          "0.3.0",
    "kind":             "ox-skill",
    "manifest_blob_id": "b3:8f7a...",
    "manifest_url":     "https://blobs.example/b3/8f/7a/...",
    "layers": [
        {
            "diff_id":           "b3:cc...",
            "blob_id":           "b3:ff...",
            "url":               "https://blobs.example/b3/ff/...",
            "size_compressed":   4123,
            "size_uncompressed": 18432
        }
    ],
    "published_at": "2026-03-20T14:22:11Z",
    "publisher":    "ox-community"
}
```

The client fetches `manifest_url` and each `layers[].url` via plain
HTTP `GET`. Verification is two-layer:

1. **Manifest.** Hash the fetched bytes → must equal
   `manifest_blob_id`. Parse. For each layer in the manifest, the
   declared `diff_id` must appear in the registry response's
   `layers` array.
2. **Each layer blob.** Hash the fetched bytes → must equal the
   registry record's `blob_id`. Then decompress and hash again →
   must equal the registry record's `diff_id`, which must match
   the manifest's `diff_id`. Both hashes are checked; neither the
   registry nor the blob backend needs to be trusted with content
   integrity.

A compromised registry can redirect to a malicious blob URL, and
the client will notice at the `blob_id` check. A malicious blob URL
that happens to match the `blob_id` but decompresses to different
content is impossible — the `diff_id` check catches it. A
compromised registry that lies about the `blob_id` is caught at
the manifest's `diff_id` boundary.

### Skipping fetches via diff_id

A client that already has a blob with the requested `diff_id` (in
any encoding) skips the fetch entirely. It does not matter which
publisher's encoding it has — the logical layer is the same, and
the local `diffs/diff_id → blob_id` index resolves to whichever
blob is already on disk. This is the property that makes the
manifest-diff_id-only design pay off: dedup across publishers and
across compression choices happens automatically.

### Version listing

```
GET /api/v1/packages/<namespace>/<name>
```

Returns the list of published versions, newest first. Clients use
this during resolution for range constraints (`^1.0`, `*`). The list
is small and bounded — no pagination for v1; we will add it if any
package ever has thousands of published versions.

### Semver resolution

The registry does not resolve semver ranges. The client does (see
[resolver.md](resolver.md)). The registry simply lists what exists.
This keeps resolution logic in one place and lets the registry stay
a dumb lookup.

---

## Namespaces and Publishers

A **namespace** is a scope inside the registry. A **publisher** is
an authenticated identity authorized to push to one or more
namespaces.

| Namespace example | Publisher | Scope |
|-------------------|-----------|-------|
| `ox-community/*` | `ox-community` (verified org) | Community packages |
| `dragon/*` | `dragon` (individual) | Personal packages |
| `acme-corp/*` | `acme-corp` (org) | Internal packages |
| `debian/*` | reserved for apt importer | Not directly publishable |

Namespace ownership is claimed at registry signup. Verified
namespaces (organizations, individuals with confirmed identity) get
a badge; unverified namespaces do not. The registry does not
adjudicate disputes; namespace squatting is an operator concern,
not an engine concern.

### Visibility

Packages are either **public** (visible to any client) or **private**
(visible only to members of the publishing namespace). Private
packages use the same API with authentication required for `GET` as
well as publish. A private package can depend on a public one; a
public package cannot depend on a private one (the registry rejects
the manifest on publish if it would).

---

## Trust

The registry stores a publisher identity per version. That is the
minimum trust primitive. Optionally, a publisher can attach a
signature over the manifest hash:

```
signature = sign(publisher_key, manifest_hash)
```

Clients that care verify the signature against the publisher's
known public key. Clients that don't, don't. Signatures are
advisory; content integrity is already guaranteed by hash matching.

Skill and hook trust is **not** a registry concern. The registry
tells clients what exists and where; it does not audit what a package
does. A consumer that cares whether a hook is safe to run is the one
who decides whether to run it. See [layers.md](layers.md) on hooks
and [consumers.md](consumers.md) on kind-specific policy.

---

## Self-Hosting

Any operator can run their own registry. The API is the contract; the
implementation is not. A minimal self-hosted registry is:

- An HTTP server implementing the endpoints below.
- A blob store (directory, S3 bucket, CDN origin).
- A small database mapping `(namespace, name, version)` to records.
- An auth layer (OAuth, API tokens, SSO integration).

An organization that wants internal elu packages points its clients
at its own registry via `$ELU_REGISTRY` or the CLI's `--registry`
flag. Clients can be configured with a fallback chain:

```
ELU_REGISTRY=https://registry.acme-corp.internal,https://registry.elu.dev
```

A reference like `acme-corp/internal-tool` is looked up against each
registry in order until one returns a result. Hash references bypass
the registry entirely and can be fetched from any registry that has
the blob.

---

## Offline Operation

The registry is optional. A user who has manifests and blobs in
their local store can resolve, stack, and output without ever
contacting it. The `--offline` flag on the CLI and resolver enforces
this explicitly (see [resolver.md](resolver.md)).

A team that vendored their dependencies into a shared local store
(via `elu stack --no-output` or explicit fetch, followed by file
sync) can work in an air-gapped environment indefinitely.

---

## HTTP API Summary

```
# Publish
POST   /api/v1/packages/<ns>/<name>/<version>           begin publish
POST   /api/v1/packages/<ns>/<name>/<version>/commit    finalize publish

# Fetch
GET    /api/v1/packages/<ns>/<name>                     list versions
GET    /api/v1/packages/<ns>/<name>/<version>           package record

# Search
GET    /api/v1/search?q=<query>&kind=<kind>&tag=<tag>   search index

# Namespaces
GET    /api/v1/namespaces/<ns>                          namespace info
POST   /api/v1/namespaces/<ns>                          claim (auth'd)
```

All endpoints return JSON. Authentication for publish endpoints is a
bearer token; authentication for private package reads is the same.
Public reads are anonymous.

---

## Non-Goals

**Not a blob host.** The registry points at blobs; it does not store
them in its primary database. Operators provide blob storage
separately.

**Not a resolver.** Semver range resolution is client-side.

**Not a signing authority.** Publisher keys are managed externally.
Signature verification policy is client-side.

**Not a mirror of upstream ecosystems.** The registry does not mirror
apt, npm, or pip. Imported packages live in the publisher's own
namespace, published by whoever ran the importer.

**Not a CI system.** The registry does not build packages. It accepts
already-built packages from clients that built them locally or in
their own CI.

**Not a social network.** No stars, no follows, no comments in v1.
Discovery is via search and publisher browsing. If a community
platform is needed later, it lives above the registry, not inside it.
