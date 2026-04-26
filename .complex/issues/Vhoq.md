## Scope

Wire \`elu refs rm <name>\` to \`Store::remove_ref\`.

Stub: \`cmd/refs.rs:55\`. PRD: \`docs/prd/cli.md:366-376\`.

## Lower-ring dep

\`WKIW.3idQ.KbJk\` (Store::remove_ref) — must land first.

## Slice

1. **Red** — CLI test: \`elu refs add\` then \`elu refs rm\` — \`elu refs ls\` no longer shows it.
2. **Green** — call \`remove_ref\`, surface the typed not-found error as a non-zero exit with a clear message.