## Scope

\`docs/design/authoring.md:23\` lists \`include\`, \`exclude\`,
\`strip\`, \`place\` as the four fields the walker resolves
safely against the project root, rejecting absolute paths and
\`..\`.

Current impl (\`crates/elu-author/src/walk.rs:27-43\`) checks
only \`include\` and \`exclude\`. \`apply_strip_place\` at line
104 just concatenates the prefix string verbatim:
\`\`\`rust
match place {
    Some(pfx) => {
        let mut s = pfx.to_string();
        if !s.ends_with('/') && !after_strip.is_empty() {
            s.push('/');
        }
        s.push_str(after_strip);
        s
    }
    ...
}
\`\`\`

So:
- \`place = \"/etc/foo/\"\` produces tar entries with absolute
  layer paths.
- \`place = \"../escape/\"\` produces tar entries that escape
  staging once unpacked.
- \`strip = \"/abs/\"\` — less dangerous (it just doesn't match)
  but still inconsistent with the include/exclude rules.

## Why this matters

A malicious or buggy manifest can today produce a tar layer
that, when unpacked by a downstream consumer, writes to
arbitrary host paths. The walker is the build-time guard; the
manifest validator could enforce the same rules even earlier.

## Slice

1. **Red** — three failing tests:
   - \`place\` starting with \`/\` rejected at build time
   - \`place\` containing \`..\` segment rejected
   - \`strip\` starting with \`/\` rejected (consistency with
     include/exclude)
2. **Green** — extract a \`reject_unsafe_layer_path(field, s)\`
   helper that does the absolute / dotdot checks; call it on
   \`layer.strip\` and \`layer.place\` at the top of
   \`walk_layer\`, alongside the existing include/exclude loop.

## Decision

Validation lives in:
- (a) \`elu-manifest::validate\` — fail at parse, never reach the
  walker, also catches it for \`elu check\`.
- (b) \`elu-author::walk\` — fail at build time only.

Prefer (a). \`elu check\` should refuse the manifest before any
filesystem work happens. Keep walk.rs's checks too as a
defense-in-depth no-op.

## Files

- \`crates/elu-manifest/src/validate.rs\` — add layer.strip /
  layer.place rejection.
- \`crates/elu-author/src/walk.rs\` — mirror the check (cheap).
- Manifest validator tests + walker tests.

## Out of scope

- Reflowing the \`apply_strip_place\` function itself — once
  inputs are validated, the existing string concat is fine.