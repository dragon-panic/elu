# Dependency resolver

Turn references (`namespace/name@constraint`) into a pinned,
deduplicated, ordered layer list ready for stacking. Deterministic
given the same store, registry, and lockfile.

**Spec:** [`docs/prd/resolver.md`](../docs/prd/resolver.md)

## Key decisions (from PRD)

- Three reference forms: name only, name + semver, name + exact hash.
  Hash references bypass version resolution.
- One-pass, fail-loudly-on-conflict. No SAT solving, no backtracking.
  Rule 3/4: `n` is small; fancy solvers are not justified.
- Lockfile (`elu.lock`) is the serialized resolution, committed to VCS.
  `--locked` refuses to proceed if the lockfile would change.
- Lockfile consulted before registry; mismatches are errors, not
  auto-updates. `elu update` moves the lock forward explicitly.
- Resolution produces a `fetch_plan` of (hash, url) pairs. The caller
  executes the fetch; resolution itself is network-free beyond version
  listing.
- `--offline` skips the registry entirely and resolves from local
  store + refs.

## Acceptance

- Standard semver ranges (`^`, `~`, comparison, exact, wildcard,
  intersection). Pre-releases excluded unless explicit.
- `resolve(roots)` returns ordered manifests, deduplicated layers,
  and a fetch plan.
- `lock()` and `verify()` operate on a manifest + lockfile pair.
- Conflict errors name every offending chain.
- `update(names)` re-resolves just those names transitively.
