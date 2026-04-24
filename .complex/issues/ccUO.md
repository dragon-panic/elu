## Scope

`parse_manifest_bytes` exists in two places:

- `crates/elu-registry/src/server/publish.rs`
- `crates/elu-registry/src/client/publish.rs`

Both currently try `serde_json::from_slice` first, then fall back to `elu_manifest::from_toml_str`. The TOML branch only exists to keep `client_publish.rs` (which seeds TOML) green — it's test-driven compat code in production parsers.

Once GjSE lands, `client_publish.rs` will go through `elu build` and produce JSON. The TOML branch becomes dead code.

## Target

Both `parse_manifest_bytes` helpers become a single line:

```rust
serde_json::from_slice::<Manifest>(bytes)
    .map_err(|e| RegistryError::InvalidManifest { reason: format!("invalid manifest: {e}") })
```

Or fold inline into the call site if it's now small enough not to warrant a helper.

## Files

- `crates/elu-registry/src/server/publish.rs`
- `crates/elu-registry/src/client/publish.rs`

Drop any `from_toml_str` / `std::str::from_utf8` usage that only existed for the TOML branch.

## Verification

Full `cargo test -p elu-registry` + `cargo test -p elu-cli` must still pass with the TOML branch removed. If anything fails, the failure proves there was still a TOML-seeded path; that path needs to be retrofitted (probably means GjSE was incomplete) before this slice can land.

## Dependency

Blocked by GjSE.

## Red/green

Trivial slice. Single `refactor — strip TOML fallback from publish parsers` commit is fine.
