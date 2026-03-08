## Registry (publish/fetch)

Content-addressed remote store. Packages are pushed/pulled by hash.

### Design
- Simple HTTP API: PUT /blob/<hash>, GET /blob/<hash>, GET /index/<name>
- Index maps name+version → hash
- Could start as a static file host (S3, R2) + JSON index
- Signing: packages signed with ed25519, verified on fetch

### Acceptance
- `elu publish <pkg>` uploads tarball + manifest to registry
- `elu fetch <name>@<version>` downloads to local store
- Hash verification on fetch (reject tampered packages)
- v1: local filesystem "registry" for testing
