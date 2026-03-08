## Importers (apt, npm, pip)

Bridge existing ecosystems into elu's universal format.

### Importer trait
```rust
trait Importer {
    fn fetch(&self, name: &str, version: &str) -> Result<TempDir>;  // download
    fn manifest(&self, name: &str, version: &str) -> Result<PackageManifest>;  // generate
    fn pack(&self, fetched: &Path) -> Result<PathBuf>;  // tarball
}
```

### apt importer
- `apt download <pkg>` → extract .deb → repack data.tar as elu tarball
- Parse Depends from control file → elu deps
- Cache downloaded .debs

### npm importer
- `npm pack <pkg>` → already a tarball, repack with elu structure
- Parse dependencies from package.json → elu deps

### pip importer
- `pip download --no-deps <pkg>` → wheel/sdist
- Unpack wheel → repack as elu tarball
- Parse Requires-Dist from METADATA → elu deps

### Acceptance
- `elu import apt curl` → package in store with correct deps
- `elu import npm @anthropic-ai/claude-code` → package in store
- `elu import pip playwright` → package in store
- Re-import same version is a no-op (hash match)
