## Scope (vertical slice 5 of 5 — closes the package-manager workflow)

Drop the single-ref guard in `install.rs:41-46`, and have `install` drive the resolver against a registry-backed source (not just `OfflineSource`) so transitive deps get fetched. Retire the stale "transitive registry resolution not yet implemented" branch at `install.rs:77-85`.

## Why

PRD: `docs/prd/cli.md:36-48` describes `install` taking `<ref>...` (variadic) and resolving transitively from the registry. Today the install command is a single-shot fetch+stack and explicitly errors if the resolver wants more blobs than the one root. The codex review on 2026-04-25 flagged this as the headline v1 gap.

## Files

- `crates/elu-cli/src/cmd/install.rs` (edit — accept N refs, build a registry source for the resolver, fetch closure into store)
- `crates/elu-resolver/src/source.rs` (likely edit — add a `RegistrySource` impl, or wire `OfflineSource` to fall back to a registry callback; whichever is the smaller change. Probably a new module `crates/elu-resolver/src/registry_source.rs` is cleaner.)
- `crates/elu-cli/tests/install_multi.rs` (new — installs a package whose deps must be fetched transitively)

## Acceptance

- `elu install foo/a@^1 foo/b@^2` resolves and stacks both, with deps of each fetched into the store.
- `elu install pkg-with-deps` (single ref, but with transitive deps) populates the store with the full closure and stacks successfully — the error at `install.rs:77-85` is gone.
- `--locked` propagates: errors if any newly-required pin is missing from the lockfile.
- Round-trip integration test from `WKIW.SnIt` still passes.
