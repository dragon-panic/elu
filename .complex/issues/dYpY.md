## Scope (vertical slice)

Add `tests/determinism.rs` with two tests that prove the build/stack pipeline is deterministic across fresh stores.

## Files

- `crates/elu-cli/tests/determinism.rs` (new)
- Reuses `tests/common/mod.rs` from slice pF9d

## Tests

1. `build_manifest_hash_is_stable` — run `build` against the same fixture into two fresh stores (separate `Env` instances), assert both `manifest_hash` values equal.
2. `tar_output_is_byte_identical` — same as (1), then `stack -o out.tar` from each store, assert SHA-256 of the two tar files equal.

Both `#[cfg(unix)]`.

## Known risk — likely red on first run

Test 2 will **probably fail immediately**. That is not a test bug; the test is finding real pipeline nondeterminism (likely mtime passthrough, iteration order, or mode passthrough). When it reds:

1. Commit the red.
2. Open a new cx task: "Pipeline: make tar output byte-deterministic" under WKIW.
3. Block this slice's green on that new task.
4. Do NOT bundle the pipeline fix into this slice.

This preserves the vertical-slice discipline and keeps the test commit reviewable on its own.

## Red/green

- Red: both tests written, at least one failing for the right reason. Commit `red — …`.
- Green: once pipeline fixes land, this slice's green is just confirming the tests now pass. Commit `green — …`.
