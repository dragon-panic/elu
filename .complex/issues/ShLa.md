## Scope

\`docs/prd/hooks.md:166\` explicitly permits absolute symlink
targets: \"an absolute target is allowed but it refers to the
path as seen at runtime in the materialized output, not to
anywhere on the host.\"

Current validator
(\`crates/elu-manifest/src/validate.rs:137-140\`):
\`\`\`rust
HookOp::Symlink { from, to, .. } => {
    reject_absolute_path(index, from)?;
    reject_absolute_path(index, to)?;
}
\`\`\`

The runtime executor
(\`crates/elu-hooks/src/ops/symlink.rs:9\`) already gets this
right — it deliberately doesn't resolve \`to\`, comment says
\"symlink targets are relative-to-link or absolute-at-runtime.\"

So the validator is rejecting a feature the runtime supports
and the PRD requires.

## Slice

1. **Red** — manifest validator test: a symlink op with
   \`to = \"/usr/bin/python3\"\` validates clean.
2. **Green** — drop the \`reject_absolute_path(index, to)?\`
   call. Keep the one for \`from\` (the link path itself, which
   IS rooted at staging).

## Files

- \`crates/elu-manifest/src/validate.rs:139\` — one-line removal.
- Validator tests — add the absolute-target case.

## Out of scope

- Symlink target safety beyond \"don't write outside staging\"
  (the runtime ops already cap that for \`from\`). Targets
  resolve at runtime, not build time, so we genuinely cannot
  pre-check them against host filesystem layout.