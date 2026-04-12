# Importers

Importers bridge external package ecosystems into elu. An importer takes
an external reference (a Debian package name, an npm package name, a pip
requirement) and produces a native elu package: a manifest, a set of
layer blobs, and entries in the store.

The rule importers live by: **the output is an ordinary elu package.**
An imported Debian package is the same shape as a hand-authored one. It
goes through the same resolver, the same stacker, the same outputs.
Consumers reading it do not know or care that it came from an importer
— they see a manifest with `kind = "debian"` and a layer list, and that
is all.

v1 ships three importers: `apt`, `npm`, and `pip`. More can follow, but
the architecture is set by the first three.

---

## Common Shape

Every importer exposes the same interface:

```
importer.import(external_ref, *, store, options) -> manifest_hash
```

Internally, the pipeline is always:

```
1. Fetch the upstream artifact (a .deb, a .tgz, a .whl).
2. Resolve upstream dependencies.
3. For each resolved upstream package:
     a. Extract its file tree.
     b. Build a tar layer.
     c. Put the layer in the store.
     d. Build a manifest referencing the layer plus elu-level deps.
     e. Put the manifest in the store.
     f. Write a ref at `refs/<importer-ns>/<name>/<version>`.
4. Return the top-level manifest hash.
```

Each imported package becomes one elu package with one layer. Upstream
dependency relationships become elu `[[dependency]]` edges between
those packages. The elu dependency graph of imported content mirrors
the upstream dependency graph, so elu's resolver can compose imported
packages with hand-authored ones without any special case.

### Why not one big layer per install

A naive importer could resolve the whole upstream closure, merge it
into one giant layer, and ship a single elu package. This would work
and would be simpler. It is the wrong default because:

- **Dedup is lost.** Two different installs that share half their
  transitive closure produce entirely different layers.
- **Updates rebuild everything.** Bumping one leaf package rebuilds
  the big layer.
- **Per-package metadata is lost.** Licenses, provenance, upstream
  version — the information consumers care about — is erased.

Keeping the 1:1 mapping (one upstream package → one elu package)
preserves dedup and provenance at the cost of more manifests in the
store. Manifests are small; we have the budget.

### Caching

Importers cache upstream tarballs and metadata. Re-importing the
same `curl@8.1.2-3` twice fetches the deb once. The cache lives
alongside the store and is managed by the same GC rules — a cached
upstream tarball whose resulting layers are no longer reachable is
eligible for collection.

---

## The `apt` Importer

Imports Debian/Ubuntu packages from an apt repository.

### Usage

```
elu import apt curl
elu import apt curl=8.1.2-3 --dist=bookworm
elu import apt --closure curl jq ripgrep
```

The `--closure` form resolves the full dependency closure with
`apt-get`'s resolver and imports every package in it. Without
`--closure`, only the named package is imported and its upstream
dependencies are left as unresolved elu `[[dependency]]` entries that
the user must either import separately or replace with hand-authored
equivalents.

### Namespace

Imported Debian packages live under the `debian` namespace:

```
debian/curl @ 8.1.2-3
debian/libssl3 @ 3.0.11-1~deb12u2
```

The exact distribution (`bookworm`, `jammy`) is carried in
`[metadata.apt]` alongside source URLs and package checksums.

### Layer construction

For each .deb:

1. Extract `data.tar.*` into a staging directory.
2. Pack the staging directory as a plain tar.
3. Put the tar as a layer in the store.
4. Build an elu manifest:
   - `namespace = "debian"`, `name` and `version` from the deb
   - `kind = "debian"`
   - `description` from the deb's `Description:` field
   - one `[[layer]]` entry
   - one `[[dependency]]` entry per `Depends:` relation
   - `[metadata.apt]` carrying the original control file
5. Put the manifest in the store and write the ref.

`postinst` scripts are **not** executed. That is a host-side concern
for the consumer of the stack, not a property of the elu package. A
consumer that needs `postinst` semantics runs them itself in its own
environment; the elu layer is just the files.

### Kind

`kind = "debian"`. Consumers that know what to do with Debian
packages (e.g. a qcow2 output producing an OS image) dispatch on this.
Consumers that do not simply treat it as "some files."

---

## The `npm` Importer

Imports npm packages from the npm registry.

### Usage

```
elu import npm lodash
elu import npm lodash@4.17.21
elu import npm --closure express
```

### Namespace

```
npm/lodash @ 4.17.21
npm/@scoped/package @ 1.0.0
```

Scoped packages preserve the `@scope/name` form inside the name
field. The elu `namespace` is still `npm`.

### Layer construction

For each tarball:

1. Fetch the package tarball from the npm registry.
2. Unpack it.
3. Pack as a layer; the layer contains the package rooted at
   `<name>/` so stacking multiple npm packages into the same target
   produces a conventional `node_modules`-shaped tree.
4. Build a manifest with dependencies mirrored from the `package.json`
   `dependencies` table.
5. Carry `[metadata.npm]` with the original `package.json`.

### Kind

`kind = "npm"`.

### Dev dependencies, optional dependencies, peers

v1 imports runtime `dependencies` only. `devDependencies`,
`optionalDependencies`, and `peerDependencies` are recorded in
`[metadata.npm]` but are not turned into elu `[[dependency]]` edges.
A consumer that needs them can read the metadata and re-import
explicitly. This is a deliberate simplification — auto-closure for
optional and peer deps pulls in the full npm resolution complexity
we are trying to avoid in [resolver.md](resolver.md).

---

## The `pip` Importer

Imports Python wheels from PyPI.

### Usage

```
elu import pip requests
elu import pip requests==2.31.0
elu import pip --closure flask
```

### Namespace

```
pip/requests @ 2.31.0
```

Normalized name rules follow PEP 503 (lowercase, non-alphanumerics
collapsed to hyphen).

### Layer construction

Wheels are zip files with a known layout. The importer:

1. Fetch the wheel for the resolved version (platform-specific
   wheels are selected based on target tags; the default target is
   `py3-none-any` with fallbacks).
2. Unzip into staging.
3. Pack as a tar layer rooted at `site-packages/<package>/`.
4. Build a manifest; dependencies come from the wheel's
   `METADATA` `Requires-Dist` fields.
5. Carry `[metadata.pip]` with wheel tags and the original METADATA.

Source distributions (`sdist`) are not supported in v1. Users who
need sdists build their own wheels first and feed them in via a
future `elu import wheel <file>` command.

### Kind

`kind = "pip"`.

---

## Composition With Hand-Authored Packages

The whole point of preserving the 1:1 mapping is that an imported
package is citizen-equal with a hand-authored one. A hand-authored
skill can depend on `debian/psql`:

```toml
[package]
namespace = "ox-community"
name      = "postgres-query"
kind      = "ox-skill"

[[dependency]]
ref     = "debian/psql"
version = "^15"
```

When the resolver walks this, it consults the `debian` namespace the
same way it consults any other — via refs in the local store or via
the registry. `elu lock` pins a hash. `elu stack` materializes the
combined tree. The `ox-skill` kind means ox-runner places the
staging tree onto PATH; the fact that some of that tree came from a
Debian package is invisible to ox-runner, which is correct.

---

## Interface Sketch

All three importers expose the same shape:

```
apt.import(name, *, version=None, dist="bookworm", closure=False, store, registry)
    -> manifest_hash

npm.import(name, *, version=None, closure=False, store, registry)
    -> manifest_hash

pip.import(name, *, version=None, target="py3-none-any", closure=False, store, registry)
    -> manifest_hash
```

Return value is the hash of the top-level imported package's
manifest. If `closure=True`, all transitively imported packages are
written to the store as a side effect; the returned hash is still
only the top-level one.

CLI wraps these as `elu import apt`, `elu import npm`, `elu import
pip`. See [cli.md](cli.md).

---

## Non-Goals

**No upstream publishing.** Importers pull from upstream registries;
they do not push back. elu does not become a mirror or a proxy for
apt / npm / pip.

**No build-from-source.** Importers consume pre-built artifacts
(.deb, tarball, wheel). Users who need source builds do them with
the upstream tooling and import the result.

**No upstream security tracking.** elu does not know which CVEs
affect which imported packages. Operators who need that use upstream
tooling (`apt-get upgrade --list`, `npm audit`, `pip-audit`) on the
upstream ecosystems and re-import.

**No cross-ecosystem resolution.** The apt importer does not know
about npm. The npm importer does not know about pip. A package that
straddles ecosystems (e.g. a Python library with a C extension that
needs libfoo) is the author's problem to express as an elu dependency
edge.

**No importer for every ecosystem in v1.** apt, npm, pip are the
opinionated starting set. More importers (cargo, gem, go modules,
OCI) are additive and do not require changes to the manifest, store,
or resolver.
