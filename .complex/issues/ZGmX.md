## Scope (small follow-on to slice 5)

Drop the single-ref guard in `stack.rs:22-27` so `stack` accepts `<ref>...` and resolves the closure of all roots together. Reuses the registry-source plumbing from slice 5.

## Why

PRD: `docs/prd/cli.md:84-117`. Stack and install share resolver shape; whatever multi-ref plumbing slice 5 introduces should drop into stack with a small follow-on edit.

## Files

- `crates/elu-cli/src/cmd/stack.rs` (edit — accept N refs)
- `crates/elu-cli/tests/stack_multi.rs` (new — or extend existing stack tests)

## Acceptance

- `elu stack foo/a@^1 foo/b@^2 -o ./out` stacks the union of both closures into `./out`.
- Deterministic apply order is preserved (test asserts stable digest of the output dir).
- qcow2/tar paths still work for single-ref invocations.
