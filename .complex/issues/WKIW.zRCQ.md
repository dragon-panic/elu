# Layer unpacking and stacking

Turn stored blobs back into a usable filesystem. A layer is a plain
tar stream representing a file tree; a stack is an ordered list of
layers applied into a staging directory with later-wins semantics,
followed by an optional post-unpack hook.

**Spec:** [`docs/prd/layers.md`](../docs/prd/layers.md)

## Key decisions (from PRD)

- Layer format is plain tar (no compression, no JSON descriptors,
  not OCI). Hash is over the plain-tar bytes.
- Whiteouts follow OCI convention: `.wh.<name>` deletes, `.wh..wh..opq`
  makes a directory opaque. Consumed during stacking, never
  materialized.
- Stack order matters; later layer wins on path collision.
- Unpack strategies: `copy`, `reflink` (default), `hardlink` (read-only
  targets only). Store is never modified.
- Staging directory is always on the host. Hook runs there, host-side,
  with `cwd = staging`, `ELU_STAGING` set, stdin `/dev/null`, 60s
  default timeout. Hook failure rolls back the stack.
- `flatten(manifest)` walks dependencies depth-first and produces the
  deduplicated layer hash list the stacker consumes.

## Acceptance

- `apply(layer, target)` handles files, dirs, symlinks, whiteouts,
  opaque whiteouts.
- `stack(manifest, target)` produces a merged tree matching manifest
  layer order.
- Reflink is used when the filesystem supports it; fallback to copy
  otherwise.
- Hook is run exactly once per stack, after all layers applied,
  before output finalization.
- Failed hook leaves the target untouched (staging dir removed).
