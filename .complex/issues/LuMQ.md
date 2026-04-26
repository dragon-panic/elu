## Scope

\`docs/design/store.md:401\` specifies that fsck's diff/ check
\"checks the referenced blob_id exists in objects/, **and that
decompressing it reproduces the diff_id in the path**\" — i.e.
the diff index entry is wrong if the referenced blob's
decompressed content hashes to something other than the diff_id
in the diffs/ filename.

Current impl (\`crates/elu-store/src/fs_store.rs:613-630\`) only
checks \`blob_path(&bid).exists()\`. A corrupted diff index
pointing to a *different but valid* blob passes silently.

## Why this matters

The diff_id → blob_id mapping is the seam between \"what does
the manifest say this layer is\" (diff_id) and \"what bytes do
we actually store\" (blob_id). If that mapping ever lies, every
downstream consumer — gc reachability, install layer assembly,
publish — operates on the wrong content. fsck is the only place
this gets caught; it has to actually look.

## Approach

1. **Red** — add an fsck test that:
   - Puts blob A (decompresses to diff_id_a) and blob B (different content)
   - Manually overwrites \`diffs/<diff_id_a>\` to point to B's blob_id
   - Asserts \`fsck()\` returns a new \`FsckError::DiffMismatch\` (or extends
     \`OrphanedDiff\` — pick a name that distinguishes \"blob missing\"
     from \"blob present but wrong content\").
2. **Green** — in fsck step 2, when the referenced blob exists,
   stream it through the decompressor and recompute the
   diff_id; compare to the path. Reuse the encoding sniffing
   path already in \`put_blob\`.

## Files

- \`crates/elu-store/src/store.rs\` — possibly add a new
  \`FsckError\` variant.
- \`crates/elu-store/src/fs_store.rs\` — fsck step 2.
- \`crates/elu-store/tests/fsck.rs\` — new test.
- \`crates/elu-store/src/fs_store.rs\` fsck_repair — decide whether
  this new variant is repairable. \"Wrong diff index\" is
  recoverable by deleting the diffs/ entry; the next put_blob
  for that diff_id will rebuild it. Treat it like OrphanedDiff.

## Out of scope

- Rebuilding correct diffs/ entries from scratch (not the job
  of \`fsck --repair\`; users re-run \`build\` to re-derive them).
- Performance of fsck on large stores — fsck is admin-only.