## Scope (vertical slice 2 of 5)

Implement `elu add <ref>...`: append a dependency to `./elu.toml`, re-run the resolver, and write the refreshed `./elu.lock`. Does **not** stack.

## Why

PRD: `docs/prd/cli.md:50-57`. Currently dispatched to `stub::run` in `cmd/mod.rs:29`. The resolver and lockfile lifecycle (slice 1) provide everything the implementation needs.

## Files

- `crates/elu-cli/src/cmd/add.rs` (new)
- `crates/elu-cli/src/cmd/mod.rs` (rewire `Command::Add` from `stub::run` to `add::run`)
- `crates/elu-cli/tests/add.rs` (new)

## Acceptance

- `elu add foo/bar@^1.2` in a directory with `elu.toml` appends a `[[dependency]]` entry and writes/refreshes `elu.lock` with `foo/bar` at a satisfying version.
- Idempotent: `elu add` of an already-present ref is a no-op (no manifest churn) but still re-locks if the lockfile is missing.
- `--locked` errors (exit 7) if the resulting lockfile would differ from disk (e.g. CI guarding against accidental adds).
- Manifest serialization preserves field order and comments where reasonable (or, if not, the test asserts the canonical re-emit shape).
