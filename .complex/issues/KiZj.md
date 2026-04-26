## Scope

Implement \`elu schema --yaml\`: emit the manifest schema in YAML alongside the existing JSON form.

Stub: \`cmd/schema.rs:10\`. PRD: \`docs/prd/cli.md:207-218\`.

## Slice

Pure CLI — JSON → YAML conversion.

1. **Red** — test: \`elu schema --yaml\` output round-trips back to the same JSON the default form produces.
2. **Green** — pull in serde_yaml (or equivalent already in the workspace), serialize the schema struct.

## Out of scope

- Schema content changes — this is a format flag only.