## Goal

A small, fast, portable integration-test layer that exercises elu's CLI as a black box and catches regressions at crate seams. "Fast tier" = pure-Rust, no external binaries, no network — runs on `cargo test`.

## Design decisions

- Tests live inside `crates/elu-cli/tests/` (not a new crate). The `elu` binary is the seam; `assert_cmd::cargo_bin` reuses the already-built binary.
- One shared `tests/common/mod.rs` helper: `Env` (tmpdir project + tmpdir store), `tiny_fixture`, `elu`, `elu_in_project`, `elu_json_done`.
- Single canonical fixture shape (`tiny_fixture`) to keep test surface area small. Variants added lazily when a future slice needs them.
- `#[cfg(unix)]` on the whole fast tier for v1. Windows deferred.

## Portability hazards (what the tests are designed to surface)

1. Filesystem iteration order (ext4 vs tmpfs vs APFS) — determinism test will fail if layer walk isn't sorted.
2. mtime / mode passthrough into tar — determinism test will fail if not pinned.
3. Tmpdir paths leaking into `--json` output — not an issue for slices 1/2 (we assert on `manifest_hash`, not full JSON), but will matter for future snapshot tests.

## Deferred (each is a future sibling task)

- qcow2 output round-trip (gated behind `ELU_OUTPUTS_QCOW2=1`; needs `mke2fs`/`qemu-img`)
- Registry publish/pull round-trip (in-process `wiremock`)
- Resolver fixture suite (tricky manifests: diamond, conflicts, cycles)
- CLI UX snapshots via `insta` (needs path-redaction infra)
- Windows support

## Budget

Fast tier under ~2s total wall clock. If creep past that, a fixture is too big or a test is doing work that belongs in a unit test.

## Known risk

Slice 2's tar-byte-identity test may red immediately — that's not a test bug, it's the test finding real pipeline nondeterminism. If so: commit the red, open a new cx task for the pipeline fix, block slice 2's green on that fix rather than bundling.
