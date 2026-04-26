## Scope

Implement \`elu init --template <name>\`: fetch a template manifest from the registry and seed the working dir.

Stub: \`cmd/init.rs:15\`. PRD: \`docs/prd/cli.md:119-141\`.

## Lower-ring dep

Needs a registry endpoint for template lookup (or reuses manifest fetch). Decompose the registry side as a child slice if no existing endpoint fits.

## Slice

1. **Red** — test: \`elu init --template hello-rust\` against a registry fixture serving a known template hash writes the expected manifest + scaffold files.
2. **Green** — minimum registry client call + file write.

## Out of scope

- Template authoring/upload tooling — separate concern.
- Interactive prompts.