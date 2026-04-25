## Scope (slice 1 of 2)

Remove the unused `elu-registry = { path = "../elu-registry" }` line from `crates/elu-resolver/Cargo.toml:9`. No `*.rs` file in `crates/elu-resolver/src/` imports anything from `elu_registry` (verified 2026-04-25 via `grep -rn 'elu_registry\b'`). The edge is dead weight from earlier scaffolding.

## Files

- `crates/elu-resolver/Cargo.toml` (delete one line)

## Acceptance

- `cargo check -p elu-resolver` succeeds.
- `cargo test --workspace` succeeds.
- `cargo clippy --workspace -- -D warnings` succeeds.

## Why this slice exists separately

It's a 5-minute cleanup, but it also *proves* codex's claim that resolver doesn't use registry. Doing it ahead of slice 2 verifies the architectural intent before the bigger move.
