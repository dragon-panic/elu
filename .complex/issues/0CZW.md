## Scope

Codex review on 2026-04-25 flagged that the implementation has drifted from the PRD ring model (`docs/prd/README.md:106-119`):

- `elu-resolver` declares a Cargo dependency on `elu-registry` (`Cargo.toml:9`) but **no source file imports anything from it** — dead weight, removable today.
- `elu-layers` depends on `elu-hooks` and `elu-resolver` because `elu-layers/src/stack.rs` does the full resolve→unpack→run-hooks orchestration. Per the PRD layers is ring 2; resolver is ring 4. Inversion.
- `elu-resolver::resolve` is `pub async fn` and `VersionSource` is an async trait. Less clear-cut: the design doc (`overview.md:103-128`) says "sync core, async only at registry edge" but a registry-backed source naturally wants to be async. Treat as a doc gap, not code rework.

The architecture inversion is most expensive at MqEx — that slice introduces a registry-backed `VersionSource` impl. If MqEx lands while the inversion stands, the natural home for the impl is inside `elu-resolver` (which already knows about elu-registry), cementing the drift. Cleaning up first lets the registry source live in `elu-registry` or `elu-cli` glue, where the PRD wants it.

## Slices

1. **Drop dead `elu-resolver → elu-registry` Cargo edge.** 1-line removal in `Cargo.toml`. Confirms the source claim ("never imported"), unblocks the rest. Trivial verification: `cargo check -p elu-resolver` still passes.

2. **Pull stack orchestration out of `elu-layers`.** `layers::stack` becomes tar/whiteouts/apply primitives only. The "resolve → unpack → run hooks" flow moves to `elu-cli` (or a new tiny `elu-stacker` crate above outputs/layers/hooks). Callers in `cmd/install.rs:95` and `cmd/stack.rs` stay sync; they import the orchestration helper from its new home. Removes `elu-resolver` and `elu-hooks` from `elu-layers`'s Cargo deps.

## Out of scope

- Making `elu-resolver` sync. Design doc's stated "sync core" goal versus the async trait reality is documented in `overview.md`; revisit if a concrete cost shows up. For now, update the design doc to admit resolver is async-capable (the trait wears `impl Future`) — done as part of slice 2's docs touch.

## Blocks

`MqEx` (multi-ref install + transitive registry resolution) blocks on slice 2 — the registry-backed source lands cleanly only after orchestration moves out of layers.
