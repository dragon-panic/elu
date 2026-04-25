## Scope (slice 2 of 2)

Restore the PRD ring model by making `elu-layers` own only tar / whiteouts / apply primitives. Move the resolve→unpack→run-hooks orchestration to `elu-cli` (or a small new `elu-stacker` crate; pick during proposal phase). After this slice:

- `elu-layers` Cargo deps: `elu-store` only (plus tar/zstd/etc).
- `elu-resolver` and `elu-hooks` are no longer in `elu-layers/Cargo.toml`.
- `cmd/install.rs:95` and `cmd/stack.rs` import the orchestration helper from its new home, not from `elu-layers`.
- `docs/design/overview.md:88-105` graph and prose are updated to match (and to note resolver is async-capable; the "sync core, async only at registry edge" wording is wrong today).

## Files (likely)

- `crates/elu-layers/src/stack.rs` — strip resolver/hooks knowledge; expose only `apply(layer, dest)` / `unpack(blob, into)` / whiteouts primitives.
- `crates/elu-layers/src/error.rs` — drop `HookError` import; layer errors no longer subsume hook errors.
- `crates/elu-layers/Cargo.toml` — drop `elu-hooks` and `elu-resolver`.
- `crates/elu-cli/src/cmd/install.rs`, `crates/elu-cli/src/cmd/stack.rs` — import orchestration from new location.
- New `crates/elu-stacker/` (if we choose the new-crate option) OR new `crates/elu-cli/src/stack_pipeline.rs` (if we keep it inside CLI).
- `docs/design/overview.md` — update crate graph + async-boundary prose.

## Acceptance

- All existing tests pass: `cargo test --workspace`.
- Workspace clippy clean.
- `cargo metadata` shows `elu-layers` depending only on `elu-store` among elu crates.
- Design doc graph matches the actual cargo metadata.

## Risk

This refactor touches the round-trip integration test (`tests/roundtrip.rs`) and stack-output tests. Hold green by keeping the orchestration helper signature identical — only its home changes.

## Blocks

`WKIW.wX0h.MqEx` is gated on this slice. The registry-backed `VersionSource` impl introduced by MqEx must land outside `elu-resolver` (in `elu-registry` or CLI glue), and that's only clean once orchestration is out of `elu-layers` too.
