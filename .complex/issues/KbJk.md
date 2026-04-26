## Scope

Lower-ring prep for \`elu refs rm\`. Add \`Store::remove_ref(name)\` that deletes a named ref atomically.

PRD: \`docs/prd/cli.md:366-376\`.

## Slice

1. **Red** — unit test: create ref, remove it, assert (a) lookup returns not-found, (b) the underlying blob is *not* deleted (gc owns that), (c) removing a non-existent ref returns a typed error, not a silent success.
2. **Green** — minimum impl on top of the existing ref store.

## Blocks

\`elu refs rm\` CLI slice.