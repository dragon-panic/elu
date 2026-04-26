## Scope

Wire \`elu gc --dry-run\` to \`Store::gc\`'s dry-run mode.

Stub: \`cmd/gc.rs:11\`. PRD: \`docs/prd/cli.md:348-355\`.

## Lower-ring dep

\`WKIW.3idQ.7Yrh\` (Store::gc dry-run mode) — must land first.

## Slice

1. **Red** — CLI integration test: populate a store, run \`elu gc --dry-run\`, assert output lists targets and store is unchanged.
2. **Green** — call the planning path; render output.

## Out of scope

- Output format polish — match existing \`elu gc\` reporting style.