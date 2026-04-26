## Scope

Implement \`elu build --watch\`: rebuild on filesystem change.

Stub: \`cmd/build.rs:11\`. PRD: \`docs/prd/cli.md:142-170\`.

## Slice

Pure CLI — wrap the existing build path in a notify-rs (or equivalent) watch loop.

1. **Red** — test that touching an input source triggers a second build invocation. Use a channel + a bounded wait.
2. **Green** — wire the watcher to the build command. Debounce to avoid duplicate builds on editor save bursts.

## Out of scope

- Incremental builds — \`--watch\` reruns the full pipeline; layer caching already handles unchanged inputs at the store level.
- Watching registry / lockfile changes.