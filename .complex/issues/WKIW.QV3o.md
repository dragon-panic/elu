# Seguro integration

Produce qcow2 images that seguro boots as sandboxed VMs for agent
runners. The boundary between elu and seguro is exactly one artifact:
a qcow2 file. Everything above is seguro; everything below is elu.

**Spec:** [`docs/prd/seguro.md`](../docs/prd/seguro.md)

## Key decisions (from PRD)

- No seguro-specific code in elu. The integration is the existing
  `qcow2` output (see lMYk / outputs.md). Swapping seguro for
  Firecracker or Kata tomorrow is a seguro-side change; elu is
  unaffected.
- A runner image is an ordinary elu stack: base OS (`kind = "os-base"`)
  + agent runtimes + pre-baked skills + seguro-specific config layer.
- Warm pools rely on hash-pinned lockfiles: `elu stack --locked`
  produces byte-identical images until the lock is updated. Layer
  dedup means a skill update only rebuilds that layer.
- Live skill injection: seguro can mount the host's elu store into
  the guest via virtiofs and run `elu stack --offline -o
  /run/ox/skills --format dir` at dispatch time. No blob transfer —
  the store is shared.
- elu handles: resolve, fetch, stack, produce qcow2.
- elu does **not** handle: VM lifecycle, networking, transparent
  proxy, TLS inspection, ephemeral keys, token metering, secret
  injection.

## Acceptance

- `elu stack ... --format qcow2 --base debian/bookworm-minbase`
  produces an image seguro boots cleanly.
- `--locked` rebuild is byte-identical.
- Documented example script: lock → stack → hand off to seguro.
- Works with virtiofs-shared store for runtime skill stacking.
