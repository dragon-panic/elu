## Scope

\`docs/design/store.md:230-260\` says put_blob streams bytes into
the tmp file while updating hashers incrementally. Current
implementation (\`crates/elu-store/src/fs_store.rs:276-351\`)
streams to tmp + blob_hasher fine, but ALSO accumulates every
post-peek byte into \`rest_bytes: Vec<u8>\` and then concatenates
to \`all_bytes = peek + rest_bytes\` before feeding the
decompressor. Large layers — a typical OS rootfs is 1-2 GB
compressed — get fully materialized in RAM.

## Why this matters

\`put_blob\` is on the hot path for both \`elu build\` and
publish. A 4 GB compressed layer means a >4 GB RSS spike just to
compute its diff_id. On constrained CI runners or laptops this
is a hard failure, not a slowdown.

## Approach (two viable shapes)

A. **Re-read tmp for decompression.** After the streaming write
   to tmp finishes, \`tmp.seek(0)\` and stream through the
   decompressor → diff_hasher. Two passes over disk but constant
   memory.

B. **Tee in one pass.** Wrap the source in an adapter that
   forwards every chunk to (1) tmp + blob_hasher and (2) a
   decompressor that feeds diff_hasher. One pass, constant
   memory, but trickier with the magic-bytes peek (must replay
   peek into the decompressor before continuing).

Recommend A unless benchmarks justify B's complexity (Pike rule
2 — measure first).

## Slice

1. **Red** — write a put_blob test that streams a synthetic 64 MB
   gzipped tar and asserts peak process RSS stays under, say,
   16 MB above baseline. \`procfs\` or \`mach\` calls; a portable
   shim is fine. Or simpler: assert that the function never
   allocates a Vec >1 MB in the body — a marker
   \`#[cfg(test)] static MAX_VEC_BYTES: AtomicUsize\` plus a
   custom allocator override for the test. Pick whichever ships.
2. **Green** — option A: refactor to seek+re-read tmp for the
   diff_id pass.

## Files

- \`crates/elu-store/src/fs_store.rs\` — \`put_blob\` body.
- \`crates/elu-store/tests/put_blob.rs\` — add the streaming
  assertion.

## Out of scope

- Changing the public \`Store::put_blob\` signature.
- Streaming through the rename / atomic-write step (already
  streams).