## Scope (vertical slice 3 of 5)

Implement `elu remove <name>`: strip a dependency from `./elu.toml` and re-lock.

## Why

PRD: `docs/prd/cli.md:59-61`. Currently dispatched to `stub::run` in `cmd/mod.rs:30`. Mirror of `add`; both share the manifest-edit + relock plumbing introduced in slices 1 and 2.

## Files

- `crates/elu-cli/src/cmd/remove.rs` (new)
- `crates/elu-cli/src/cmd/mod.rs` (rewire `Command::Remove`)
- `crates/elu-cli/tests/remove.rs` (new)

## Acceptance

- `elu remove foo/bar` removes the matching `[[dependency]]` entry and rewrites `elu.lock` without `foo/bar` (or any of its now-orphan transitive deps).
- Errors with a clear message if the named package is not in the manifest.
- `--locked` errors (exit 7) if the resulting lockfile would differ from disk.
