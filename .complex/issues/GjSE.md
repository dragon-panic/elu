## Scope

`crates/elu-registry/tests/client_publish.rs` currently builds its fixture by hand-writing a TOML manifest string and calling `store.put_manifest` / `store.put_ref` directly. This bypassed slice 1's bug (the publish parser only handled TOML; real `elu build` writes JSON) and is the reason production code carries a `parse_manifest_bytes` helper that accepts both formats.

Switch the test to build through `elu_author::build` (or whatever the closest equivalent is — `crates/elu-cli/tests/publish.rs` does this via `assert_cmd::Command::cargo_bin("elu").args(["build"])`; `crates/elu-cli/tests/roundtrip.rs` uses the same path). The exact API depends on whether `elu-registry`'s test code wants to depend on the `elu` binary (yes — `assert_cmd` is fine as a dev-dep) or on `elu_author` directly (also fine; library call avoids spawning a subprocess).

## Target

- Test no longer hand-writes TOML
- Test no longer calls `store.put_manifest` / `store.put_ref` directly
- Manifest in store is JSON (the canonical form `elu build` writes)
- `publish_package_end_to_end` still passes

The `make_manifest_toml` and `make_tar_bytes` helpers in the test go away. Replace with: write `elu.toml` + `layers/files/hello.txt` to a tmpdir, then either `Command::cargo_bin("elu").args(["--store", ..., "build"]).current_dir(...)` OR a direct `elu_author::build::build(...)` call.

## Files

- `crates/elu-registry/tests/client_publish.rs` (modify)
- `crates/elu-registry/Cargo.toml` (add `assert_cmd` and/or `elu-author` as dev-dep, depending on chosen path)

## Red/green

This isn't a fail-then-pass slice; it's a refactor. The test passes before and after. Single commit is appropriate (no red/green theater).

Commit message: `WKIW.<id>: refactor — client_publish test builds via real path` or similar.

## Independence

Independent of slice 6zSs (LocalBlobBackend). Either order works.

## Why this matters

Once this lands, every publish test path goes through the real JSON-manifest format, which means any future "wire format vs stored format" mismatch surfaces immediately rather than hiding behind a hand-seeded TOML fixture. This unblocks slice 3 (strip TOML fallback from production parsers).
