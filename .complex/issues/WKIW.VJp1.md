## Content-addressed store

The foundation. All packages are stored as blobs identified by sha256.

### Design
- Store location: `~/.local/share/elu/store/` (respect XDG_DATA_HOME)
- Layout: `store/sha256/<first-2-chars>/<full-hash>.tar.zst`
- Operations: put(tarball) → hash, get(hash) → tarball, exists(hash), gc()
- Deduplication is automatic — same content = same hash = stored once
- Metadata sidecar: `store/sha256/<hash>.meta.json` (name, version, source)

### Acceptance
- `elu store put <tarball>` stores and prints hash
- `elu store get <hash>` retrieves to stdout or path
- `elu store ls` lists all stored packages
- `elu store gc` removes unreferenced blobs
- Round-trip: put → get → identical bytes (verified by hash)
