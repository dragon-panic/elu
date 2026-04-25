# Dependency Resolver

The resolver turns a set of **references** (human-facing names and
version constraints) into a **resolution** (an ordered, pinned set of
manifest hashes ready to be stacked). It is the bridge between the
sloppy, human-friendly world of `ox-community/postgres-query@^0.3` and
the strict, machine-friendly world of `b3:8f7a1c2e...`.

The resolver is deterministic. Given the same references, the same
registry state, and the same local store, it always produces the same
resolution. This is what makes lockfiles meaningful.

---

## References

A reference identifies a package. It has three forms:

| Form | Example | Meaning |
|------|---------|---------|
| Name only | `ox-community/postgres-query` | Latest version. |
| Name + semver | `ox-community/postgres-query@^0.3` | Highest version satisfying the constraint. |
| Name + hash | `ox-community/postgres-query@b3:8f7a...` | That exact manifest. |

Hash references are always the most specific: they bypass version
resolution entirely and name a single manifest. Semver constraints
follow standard semver range syntax (`^1.0`, `~1.2`, `>=1.0 <2.0`,
`1.x`, `*`). Name-only references are shorthand for `@*`.

A **root reference** is one given to the resolver directly (on the CLI,
in a project config, in an API call). A **dependency reference** is one
that appears inside a manifest's `[[dependency]]` list. The resolver
treats them the same, except that root references can be made more or
less strict by user flags (`--locked`, `--update`).

---

## Resolution Pipeline

```
resolve(roots, *, lockfile=None, store=..., registry=...) -> resolution
    1. seed the work queue with root references
    2. while queue not empty:
         ref = queue.pop()
         manifest_hash = resolve_one(ref)
         manifest = fetch_manifest(manifest_hash)
         for dep in manifest.dependency:
             queue.push(dep)
    3. check for conflicts
    4. topologically order manifests
    5. flatten to layer list
    6. return resolution
```

Each step is deliberately simple. Fancy is the enemy here —
dependency resolution is where package managers go to die, and elu
has a strong preference for "resolve in one pass, fail loudly on
conflict" over "solve an arbitrary SAT problem."

### `resolve_one`

```
resolve_one(ref) -> manifest_hash:
    if ref is a hash:
        return ref.hash                 # nothing to resolve
    if lockfile and lockfile has ref.name:
        pinned = lockfile[ref.name]
        if pinned satisfies ref.version:
            return pinned               # lock honored
        else:
            error: lock conflict
    # no lock, or lock didn't apply
    candidates = registry.versions(ref.name)
    match = highest version in candidates satisfying ref.version
    if no match:
        error: no version satisfies
    return registry.resolve(ref.name, match)
```

The resolver consults the lockfile before the registry. A reference
with a lock entry that satisfies its constraint is resolved locally —
no registry round trip, no surprises. A reference whose lock entry
does **not** satisfy the current constraint is an error, not an
automatic update; the user runs `elu update` to move the lock
forward.

### Conflict check

A conflict occurs when two dependency chains require the same package
at incompatible versions. The resolver detects this after walking all
references:

```
for name, hashes in group_resolved_by_name:
    if len(hashes) > 1:
        error: conflict on <name>: <list of (chain, hash)>
```

v1 does not attempt backtracking. The error message names every
offending chain so the user can see who wants what. Resolution is the
user's problem to fix (pin one side, upgrade the other, fork); the
resolver's job is to be honest about the conflict.

This is a Rule 3/4 decision. Real backtracking resolvers are hundreds
of pages of semantics. The number of elu packages an early user has
is small. When `n` is small, "run once and yell on conflict" is both
faster in wall time and more predictable than a clever solver.

### Topological order

Manifests are topologically ordered: a manifest's dependencies come
before it. Within a tie, alphabetical order by `namespace/name`
breaks it so that resolution output is stable.

### Flatten to layer list

Once the manifests are ordered, `layers.flatten` (see
[layers.md](layers.md)) walks them in order and produces the
deduplicated layer hash list. That list is what gets passed to
`stack` to materialize an output.

---

## Lockfile

A lockfile is the serialized output of a successful resolution. It
pins every package in the resolution to an exact manifest hash.

```toml
# elu.lock
schema = 1

[[package]]
namespace = "ox-community"
name      = "postgres-query"
version   = "0.3.2"
hash      = "b3:8f7a1c2e4d3b..."

[[package]]
namespace = "ox-community"
name      = "shell"
version   = "1.1.0"
hash      = "b3:3b9e0a77f1..."
```

The lockfile is intended to be committed to version control. A team
that commits `elu.lock` gets byte-identical stacks on every machine
until someone runs `elu update`.

The lockfile lives next to the project's `elu.toml`. CLI commands
that read or write it locate the project root by walking up from
the current directory until an `elu.toml` is found — the same rule
cargo uses to find `Cargo.toml`/`Cargo.lock`. Running `elu lock`
from a subdirectory of a project updates the project-root lockfile,
not a stray subdirectory file.

### Lockfile commands

| Command | Effect |
|---------|--------|
| `elu lock` | Resolve the current manifest's roots and write `elu.lock`. |
| `elu lock --locked` | Error if the lockfile would need to change. Used in CI. |
| `elu update` | Re-resolve without consulting the lockfile; rewrite it. |
| `elu update <name>` | Re-resolve only `<name>` and its transitive deps. |

See [cli.md](cli.md) for the full CLI surface.

### Lockfile vs hash reference

A user who pins every root reference to an exact hash effectively
has an inline lockfile. A lockfile is more ergonomic because it
separates "the versions my code tolerates" (the manifest) from "the
exact versions in use today" (the lock). Both reach the same place.

---

## Version Semantics

elu uses standard semver for human-facing versions. The resolver
implements:

- **Caret (`^1.2.3`)** — compatible with 1.2.3 up to (but not
  including) 2.0.0.
- **Tilde (`~1.2.3`)** — compatible with 1.2.3 up to 1.3.0.
- **Comparison (`>=`, `<`, `<=`, `>`)** — as expected.
- **Exact (`=1.2.3` or bare `1.2.3`)** — only that version.
- **Wildcard (`*`, `1.x`, `1.2.x`)** — any version matching the
  fixed prefix.
- **Intersection (`>=1.0 <2.0`)** — both constraints must hold.

Pre-release versions (`1.0.0-rc.1`) are excluded from range matches
unless the range explicitly includes a pre-release.

**Hash references bypass semver entirely.** If a root or a dep pins
a manifest hash, the resolver skips version resolution for that
reference and verifies that the hash is fetchable.

---

## Interface Sketch

```
# Resolve a set of roots against the current store and registry
resolver.resolve(roots, *, lockfile=None, offline=False) -> resolution

# Check a lockfile against its manifest without resolving over the network
resolver.verify(manifest_path, lockfile_path) -> ok | list of errors

# Produce a new lockfile from a manifest
resolver.lock(manifest_path) -> lockfile

# Update specific packages in a lockfile
resolver.update(manifest_path, lockfile_path, names=None) -> new lockfile
```

The `resolution` struct contains:

```
resolution = {
    manifests: ordered list of (ref, manifest_hash, manifest)
    layers:    ordered list of layer_hash (deduplicated)
    fetch_plan: list of (hash, source_url)  # blobs not yet in store
}
```

The fetch plan tells the registry client exactly what blobs to pull
before stacking. Resolution never pulls implicitly — it reports what
needs pulling and returns. The caller (typically the CLI or a stack
operation) executes the plan.

---

## Offline Resolution

`--offline` skips the registry entirely. The resolver consults only
the local store's `refs/` and any manifests already present. A
reference that cannot be resolved from local state is an error. This
is the mode a runner uses after its skills have been pre-cached: the
store already has everything, so there is no need to hit the network.

A lockfile plus offline mode plus a pre-warmed store is the "dispatch
is instant" story for consumers that care about latency.

---

## Non-Goals

**No SAT solving.** One-pass, fail-on-conflict. The day this is the
limiting factor we will know because users will be complaining
specifically about backtracking, and we can revisit.

**No feature flags or conditional dependencies.** A manifest either
depends on something or it does not. Build-time conditionality
belongs in the importer or the build script, not in the manifest.

**No lockfile merging.** If two branches produce different
`elu.lock` files, the user resolves the conflict the same way they
resolve any other file merge conflict — by re-running `elu lock` on
the merged manifest.

**No dependency hoisting.** Every manifest's dependency set is
independent; there is no global "hoist" like npm's node_modules
optimization. Layer-level deduplication in the store already gives
the storage benefit hoisting aims for.
