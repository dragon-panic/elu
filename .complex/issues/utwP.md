## Scope

Implement \`elu explain --diff <old> <new>\`: human-readable diff between two manifest hashes.

Stub: \`cmd/explain.rs:11\`. PRD: \`docs/prd/cli.md:188-205\`.

## Slice

Pure CLI — both manifests are already accessible via the store.

1. **Red** — test: given two synthetic manifests differing in package set + hooks, diff output names additions/removals/changes in a stable, parseable form.
2. **Green** — load both manifests, structural diff, render. Reuse any existing manifest pretty-printer where possible.

## Out of scope

- Color / TTY heuristics beyond what \`elu explain\` already does.
- Diffing layer contents — manifest-level only here.