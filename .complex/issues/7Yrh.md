## Scope

Lower-ring prep for \`elu gc --dry-run\`. Add a dry-run mode to \`elu-store::Store::gc\` that reports what *would* be deleted without mutating the store.

PRD: \`docs/prd/cli.md:348-355\`.

## Slice

1. **Red** — unit test in \`elu-store\`: populate a store with reachable + unreachable blobs, call \`gc(dry_run = true)\`, assert (a) the returned plan names exactly the unreachable set, (b) the store is byte-identical before and after.
2. **Green** — thread a flag (or split into \`plan_gc\` + \`apply_gc\`) through the gc impl. Prefer the split — composable and trivial to test.

## Blocks

\`elu gc --dry-run\` CLI slice.