## Scope

PRD (\`docs/prd/manifest.md:24\`) says unknown manifest fields are
\"preserved but ignored by elu itself.\" Current implementation
silently drops them on parse — verified by
\`crates/elu-manifest/tests/edge_cases.rs:24\`, which documents the
drop and calls it acceptable. Direct spec mismatch.

## Why this matters

The PRD positions unknown fields as a forward-compat seam: tools
(linters, doc generators, CI helpers) attach metadata under custom
keys without forcing a manifest schema bump. If we drop on parse and
re-emit canonical JSON without those keys, every \`build\` silently
strips a third-party tool's annotations.

## Approach

1. **Red** — flip the existing \`unknown_fields_preserved_in_toml_roundtrip\`
   test to actually assert preservation: parse → re-serialize →
   confirm \`custom_top_level\`, \`custom_package_field\`, and
   \`custom_layer_field\` all survive. Update the comment to drop
   the \"acceptable\" caveat.
2. **Green** — add \`#[serde(flatten)] pub extra: BTreeMap<String, toml::Value>\`
   (or equivalent) on \`Manifest\`, \`Package\`, \`Layer\`. Skip
   serializing when empty.

## Decisions to confirm before greening

- Canonical JSON form: do unknown fields round-trip through
  \`to_canonical_json\` too? PRD implies yes (consumers reading the
  stored form should see them). If yes, the type used for \`extra\`
  must serialize cleanly to JSON — \`serde_json::Value\` is safer
  than \`toml::Value\` for round-tripping.
- Hash determinism: extra-field ordering must be canonical (sorted
  keys) so the manifest hash is stable across parsers. \`BTreeMap\`
  gives this for free.

## Files

- \`crates/elu-manifest/src/types.rs\` — add the flatten field.
- \`crates/elu-manifest/tests/edge_cases.rs\` — strengthen the test.
- Possibly \`crates/elu-manifest/src/lib.rs\` if \`to_canonical_json\`
  needs adjustment.

## Out of scope

- Preserving unknown \`[[layer]]\` array entries vs. unknown fields
  inside a known \`[[layer]]\` — handle the latter only.
- Validating that extra fields match a particular shape — they're
  by definition unknown.