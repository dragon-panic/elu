## Scope

The package-manager workflow gap (codex 2026-04-25) is tracked under `WKIW.wX0h`. This sibling node tracks every **other** "not implemented in v1" branch in the CLI that the design docs do not call out as deferred. Each is a small vertical slice; some are blocked on a lower-ring API.

## Inventory (file:line refs)

| # | Gap | Stub site | Lower-ring dep? |
|---|-----|-----------|-----------------|
| 1 | `elu init --from` (project-tree inference) | `cmd/init.rs:10` | none — pure CLI/heuristics |
| 2 | `elu init --template` | `cmd/init.rs:15` | needs registry template fetcher (small) |
| 3 | `elu build --watch` | `cmd/build.rs:11` | none — file-watcher in CLI |
| 4 | `elu explain --diff <old> <new>` | `cmd/explain.rs:11` | none — manifest already accessible |
| 5 | `elu schema --yaml` | `cmd/schema.rs:10` | none — JSON→YAML conversion |
| 6 | `elu gc --dry-run` | `cmd/gc.rs:11` | needs `Store::gc` dry-run mode |
| 7 | `elu fsck --repair` | `cmd/fsck.rs:10` | needs `Store::fsck_repair` |
| 8 | `elu refs rm` | `cmd/refs.rs:55` | needs `Store::remove_ref` |

PRD references for each: `docs/prd/cli.md:119-141` (init), `:142-170` (build), `:188-205` (explain), `:207-218` (schema), `:348-355` (gc), `:357-364` (fsck), `:366-376` (refs).

## How to use this node

Decompose into one cx slice per row before starting work — each is independent and shippable on its own. Items 6, 7, and 8 need a paired lower-ring change (in `elu-store`) that should be its own first slice for that row.

## Out of scope here

- `audit`, `policy` — punted to v1.x in `docs/design/overview.md` (they depend on the `run` capability model).
- The package-manager workflow (`add`/`remove`/`lock`/`update`/multi-ref `install`+`stack`) — tracked under `WKIW.wX0h`.
