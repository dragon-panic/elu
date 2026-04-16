# elu-manifest: Package Manifest

Implementation design for the manifest format described in
[../prd/manifest.md](../prd/manifest.md). This crate owns the
`Manifest` struct, the TOML ↔ canonical-JSON encoding, the
diff_id/blob_id/manifest-hash identity rules, validation, and the
`ManifestReader` trait the store uses during GC.

---

## Scope

- `Manifest`, `Package`, `Layer`, `Dependency`, `HookOp`, `Metadata`
  types.
- TOML parse/serialize via `serde` + `toml`.
- Canonical JSON serialization used for computing the manifest hash.
- Validation rules from the PRD.
- `ManifestReader` impl used by `elu-store::gc`.
- Source vs stored form: the same types cover both; which fields on
  `Layer` are populated distinguishes them.

Out of scope: the `elu build` pipeline that lowers source form into
stored form (lives in `elu-cli`/`authoring.md`); hook op execution
(lives in `elu-hooks`).

---

## Types

```rust
// crates/elu-manifest/src/lib.rs
use elu_store::{DiffId, ManifestHash};
use semver::{Version, VersionReq};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub package: Package,

    #[serde(rename = "layer", default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,

    #[serde(rename = "dependency", default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,

    #[serde(default, skip_serializing_if = "Hook::is_empty")]
    pub hook: Hook,

    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub namespace: String,
    pub name: String,
    pub version: Version,   // semver::Version; parses on deserialize
    pub kind: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// A layer entry. Source form populates `include` et al.; stored
/// form populates `diff_id` + `size`. Validation enforces exactly
/// one shape per entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    // --- Stored form ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_id: Option<DiffId>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    // --- Common ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    // --- Source form (consumed by `elu build`, stripped before
    //     writing stored form) ---
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Layer {
    pub fn is_source_form(&self) -> bool { !self.include.is_empty() }
    pub fn is_stored_form(&self) -> bool { self.diff_id.is_some() }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dependency {
    #[serde(rename = "ref")]
    pub reference: PackageRef,   // "namespace/name"
    #[serde(default = "default_any")]
    pub version: VersionSpec,    // VersionReq or pinned ManifestHash
}

#[derive(Clone, Debug)]
pub enum VersionSpec {
    Range(VersionReq),        // e.g. ^1.0
    Pinned(ManifestHash),     // exact hash; resolver skips version resolution
    Any,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Hook {
    #[serde(rename = "op", default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<HookOp>,
}

impl Hook { pub fn is_empty(&self) -> bool { self.ops.is_empty() } }

/// Closed set of declarative ops. See [./hooks.md].
/// v1 does NOT include `Run`. The enum will gain a `Run` variant
/// when the capability model ships; adding a variant is an additive
/// schema change at the manifest level (the `type` field is how
/// TOML distinguishes variants).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookOp {
    Chmod    { paths: Vec<String>, mode: String },
    Mkdir    { path: String, #[serde(default)] mode: Option<String>,
                             #[serde(default)] parents: bool },
    Symlink  { from: String, to: String,
               #[serde(default)] replace: bool },
    Write    { path: String, content: String,
               #[serde(default)] mode: Option<String>,
               #[serde(default)] replace: bool },
    Template { input: String, output: String,
               #[serde(default)] vars: BTreeMap<String, String>,
               #[serde(default)] mode: Option<String> },
    Copy     { from: String, to: String },
    Move     { from: String, to: String },
    Delete   { paths: Vec<String> },
    Index    { root: String, output: String,
               #[serde(default = "default_index_format")] format: IndexFormat },
    Patch    { file: String,
               #[serde(flatten)] source: PatchSource,
               #[serde(default)] fuzz: bool },
    // `Run { .. }` — deferred, see ./hooks.md
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PatchSource {
    Inline { diff: String },
    File   { from: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IndexFormat { Sha256List, Json, Toml }

/// Free-form table. Preserved verbatim, never interpreted by elu.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metadata(pub toml::value::Table);
```

Notes:

- `#[serde(tag = "type")]` on `HookOp` gives us the discriminated
  union the PRD spec shows (`type = "chmod"`, etc.). Adding a new
  variant later is additive — unknown variants are rejected at parse
  time by serde, which is what we want (the op set is closed by
  elu-side code).
- `Metadata` uses `toml::value::Table` directly so we preserve the
  user's structure without imposing a schema.
- `PackageRef` is a newtype wrapping `String` with a validating
  `FromStr` (must match `^[a-z0-9][a-z0-9-]*\/[a-z0-9][a-z0-9-]*$`).
- `VersionSpec` has a custom deserializer: if the input parses as
  `ManifestHash`, it's `Pinned`; if it parses as `VersionReq`, it's
  `Range`; if it's the string `"*"`, it's `Any`.

---

## Parsing and serialization

### TOML (source and stored form on disk)

```rust
pub fn from_toml_str(src: &str) -> Result<Manifest, ManifestError>;
pub fn from_toml_slice(bytes: &[u8]) -> Result<Manifest, ManifestError>;
pub fn to_toml_string(m: &Manifest) -> Result<String, ManifestError>;
```

Uses `toml::from_str` / `toml::to_string_pretty` via serde. TOML is
the human-facing format and the format stored in the CAS —
`elu-store` reads manifests as bytes and passes them to
`elu-manifest::parse_stored`.

### Canonical JSON (for the manifest hash)

The manifest hash is computed over a **canonical JSON serialization**
of the manifest, not over the TOML text. This is load-bearing: TOML
has formatting flexibility (quoting, whitespace, key ordering) that
would make byte equality of two logically-identical manifests
accidental. JSON with sorted keys gives us a stable, deterministic
encoding that any tool can re-derive.

```rust
/// Serialize a manifest to canonical JSON. Keys are sorted at every
/// level; strings are JSON-escaped per RFC 8259; numbers are
/// integer-only where possible. Byte-identical across platforms.
pub fn to_canonical_json(m: &Manifest) -> Vec<u8>;

/// Compute the manifest's hash: sha256 over the canonical JSON.
pub fn manifest_hash(m: &Manifest) -> ManifestHash {
    let json = to_canonical_json(m);
    let mut h = elu_store::Hasher::new();
    h.update(&json);
    ManifestHash(h.finalize())
}
```

Canonicalization rules (identical to the ones the registry uses):

1. **Object keys are sorted** lexicographically by their UTF-8 byte
   order.
2. **No insignificant whitespace.** Separators are exactly `,` and
   `:` with no surrounding spaces.
3. **Strings** use RFC 8259 escaping. Non-ASCII is passed through;
   only the required escapes (`"`, `\`, control chars) are applied.
4. **Numbers** are integers where representable; floats are not
   permitted in the manifest schema (the only numeric field is
   `size`, which is a `u64`).
5. **Optional empty collections are omitted**, not serialized as
   `[]` or `{}`. Two manifests that differ only in whether an empty
   `tags` array is present must hash to the same value.
6. **`schema` is always present and first in serialization output,
   even though keys are sorted** — actually, we rely on alphabetical
   ordering placing `schema` after `package`, which is fine.
   "Sorted" wins unconditionally; there are no special-case first
   keys.

Implementation: custom `Serializer` that wraps `serde_json::Serializer`
with a pre-pass that normalizes via `BTreeMap`. Reference: the
[JSON Canonicalization Scheme](https://datatracker.ietf.org/doc/html/rfc8785)
describes the same rules; we don't claim JCS compliance (we have
a few extra rules like "omit empty collections") but the spirit is
the same.

### Storing a manifest

```rust
pub fn put<S: elu_store::Store>(
    store: &S,
    m: &Manifest,
) -> Result<ManifestHash, ManifestError> {
    let json = to_canonical_json(m);
    let hash = store.put_manifest(&json)?;
    // Assert what we computed matches what the store computed.
    debug_assert_eq!(hash, manifest_hash(m));
    Ok(hash)
}
```

The store hashes the bytes it stores; `manifest_hash` hashes the
canonical JSON; they must agree. In debug builds we assert this.

On-disk representation: manifests are stored as **canonical JSON
bytes**, not as TOML. The `elu.toml` source file is a human
convenience; the CAS holds the JSON serialization because that's
what the hash covers. `elu inspect <hash>` renders the stored JSON
back to TOML for display, but the source of truth is the JSON.

This is a departure worth flagging: the PRD says "TOML on disk,
equivalent JSON on the wire." The design pins on-disk to JSON
because storing TOML would require a canonical-TOML form, and
canonical TOML is not a thing. JSON canonicalization is well-
understood and well-supported. The `elu.toml` file that lives in a
project directory is still TOML; the stored manifest blob in
`objects/` is JSON. Neither humans nor agents interact with the
stored form directly — they go through `elu inspect` / `elu show`
which renders TOML.

---

## Validation

```rust
pub fn validate_stored(m: &Manifest) -> Result<(), ManifestError>;
pub fn validate_source(m: &Manifest) -> Result<(), ManifestError>;
```

Both share a common core (`validate_common`) and differ only in the
per-layer checks:

`validate_common` enforces:

1. `schema` is in the supported set (currently `{1}`).
2. `package.namespace` matches `^[a-z0-9][a-z0-9-]*$`.
3. `package.name` matches `^[a-z0-9][a-z0-9-]*$`.
4. `package.version` parses as semver (enforced by the type; here
   we just accept whatever `semver::Version` deserialized).
5. `package.kind` is non-empty and has no whitespace.
6. `package.description` is non-empty and is a single line.
7. Each `Dependency.reference` is a valid `namespace/name`.
8. Each `HookOp` is well-formed (required fields present, globs
   parse, modes parse, paths are staging-relative).
9. `metadata` is valid TOML (already enforced at parse time).

`validate_stored` additionally requires:

- Every `Layer` is in stored form: `diff_id` and `size` present,
  source-form fields (`include`, `exclude`, `strip`, `place`,
  `mode`) absent.
- Each `diff_id` parses as a known algorithm.

`validate_source` additionally requires:

- Every `Layer` is in source form: `include` present, stored-form
  fields absent.
- Each `include` glob parses via `globset::GlobBuilder`.

The two validators share code via an enum parameter to the inner
function. They do not run over each other's forms silently.

**Validation does not check that referenced blobs exist in the
store.** That's a `elu-resolver` concern — the resolver builds a
fetch plan and the stack step fails if a blob cannot be made
present. `elu-manifest` is content-only.

---

## `ManifestReader` for GC

```rust
// crates/elu-manifest/src/reader.rs

pub struct ManifestReaderImpl;

impl elu_store::ManifestReader for ManifestReaderImpl {
    fn layer_diff_ids(&self, bytes: &[u8])
        -> Result<Vec<DiffId>, ManifestReadError>
    {
        let m: Manifest = serde_json::from_slice(bytes)?;
        Ok(m.layers.into_iter()
            .filter_map(|l| l.diff_id)
            .collect())
    }

    fn dependency_hashes(&self, bytes: &[u8])
        -> Result<Vec<ManifestHash>, ManifestReadError>
    {
        // Stored-form manifests have pinned dependencies (the
        // resolver wrote them as hashes). A non-pinned dependency
        // in a stored-form manifest is a bug.
        let m: Manifest = serde_json::from_slice(bytes)?;
        m.dependencies.into_iter()
            .map(|d| match d.version {
                VersionSpec::Pinned(h) => Ok(h),
                _ => Err(ManifestReadError::UnpinnedDep),
            })
            .collect()
    }
}
```

Resolver-side detail: when the resolver writes a pinned manifest
back to the store after resolution, it rewrites every dependency's
`version` to `VersionSpec::Pinned(hash)`. The "source" version range
is preserved in the lockfile for humans, not in the stored manifest.

---

## Source → Stored lowering

`elu build` lowers a source-form manifest to a stored-form manifest
by:

1. For each source-form `[[layer]]`, walk the include/exclude globs,
   collect files, apply `strip`/`place`/`mode` directives, and pack
   into an uncompressed tar stream.
2. Hash the tar to get `diff_id`. Compress with zstd (the default;
   configurable via CLI) and hand the compressed bytes to
   `store.put_blob`, which computes `blob_id` and writes the CAS
   entry.
3. Replace the source-form `Layer` with a stored-form `Layer` using
   the returned `diff_id` and the uncompressed size.
4. `validate_stored` the result.
5. `put` the manifest into the store; the returned `ManifestHash`
   is the package's identity.

The lowering pipeline lives in `elu-cli` under
`crates/elu-cli/src/build.rs` because it needs `globset`, `tar`,
`zstd`, and the filesystem — all things `elu-manifest` deliberately
doesn't touch. `elu-manifest` only owns the types, the
serialization, and the validation.

---

## Errors

```rust
#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    #[error("toml parse: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("json parse: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("schema version {0} not supported")]
    UnsupportedSchema(u32),

    #[error("invalid namespace: {0}")]
    InvalidNamespace(String),

    #[error("invalid name: {0}")]
    InvalidName(String),

    #[error("invalid package ref: {0}")]
    InvalidPackageRef(String),

    #[error("layer {index} mixes source and stored form")]
    MixedLayerForm { index: usize },

    #[error("layer {index} missing required field: {field}")]
    LayerMissingField { index: usize, field: &'static str },

    #[error("hook op {index}: {msg}")]
    HookOp { index: usize, msg: String },

    #[error("metadata must be a table")]
    MetadataNotTable,
}
```

Error codes: `manifest.toml_parse`, `manifest.json_parse`,
`manifest.unsupported_schema`, `manifest.invalid_namespace`,
`manifest.invalid_name`, `manifest.invalid_package_ref`,
`manifest.mixed_layer_form`, `manifest.layer_missing_field`,
`manifest.hook_op`, `manifest.metadata_not_table`.

---

## Open questions

- **`schema` version strategy.** v1 is schema 1. The first time we
  need a breaking change (not an additive field), we bump to schema
  2 and teach the parser to accept both. No plan today; flagged so
  we pick a migration posture when the first bump happens.
- **`size` semantics when a layer is empty.** An empty tar is
  ~1024 bytes (two zero blocks). We record that as `size = 1024`,
  not `size = 0`. Documented here so the validator doesn't get
  clever.
