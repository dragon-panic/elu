## Scope

Lower-ring prep for \`elu fsck --repair\`. Add \`Store::fsck_repair\` that detects and corrects recoverable inconsistencies (orphaned tmpfiles, unreferenced index entries, etc.) reported by \`fsck\`.

PRD: \`docs/prd/cli.md:357-364\`.

## Slice

1. **Red** — unit test per recoverable failure mode: corrupt the store in a known way, assert \`fsck_repair\` returns it to a fsck-clean state and reports what it did.
2. **Green** — implement repair for the failure modes that \`fsck\` already detects. Anything \`fsck\` flags but \`fsck_repair\` can't safely fix should return an error naming it — never silently skip.

## Blocks

\`elu fsck --repair\` CLI slice.

## Out of scope

- New fsck checks — \`fsck_repair\` repairs what \`fsck\` already finds; expanding the audit is a separate task.