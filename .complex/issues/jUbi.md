## Scope (vertical slice 4 of 5)

Implement `elu update [<name>...]`: re-resolve manifest roots ignoring lockfile pins (or for named packages only, ignoring just their pins), then rewrite `./elu.lock`. Does not stack.

## Why

PRD: `docs/prd/cli.md:73-82`. Currently dispatched to `stub::run` in `cmd/mod.rs:32`. The resolver already exposes `lockfile::update(manifest, lockfile, names)` — this slice is mostly CLI plumbing.

## Files

- `crates/elu-cli/src/cmd/update.rs` (new)
- `crates/elu-cli/src/cmd/mod.rs` (rewire `Command::Update`)
- `crates/elu-cli/tests/update.rs` (new)

## Acceptance

- `elu update` re-resolves all roots and overwrites `elu.lock`.
- `elu update foo/bar` re-resolves only `foo/bar` and its transitive deps; other packages stay pinned.
- Test: a manifest constraint that admits a newer version produces a lockfile bump.
