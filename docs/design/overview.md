# elu: Implementation Overview

Cross-cutting design decisions for elu. Per-component designs live
alongside in this directory. The Product Requirements live in
[../prd/](../prd/) and define the intended behavior; this directory
defines how we build it.

Where the PRD leaves a choice open ("TOML or JSON", "gzip or zstd",
"SQLite or flat files"), the design docs pick one, record the reason in
one or two sentences, and move on. The goal is that a reader can start
implementing from `docs/design/*` without having to rediscover any
choice that has already been made.

---

## Scope for v1

The design docs target a v1 that is narrower than the PRD in one load-
bearing way:

- **The `run` hook op and the capability-approval model are deferred
  out of v1.** v1 implements only the closed declarative op set
  (`chmod`, `mkdir`, `symlink`, `write`, `template`, `copy`, `move`,
  `delete`, `index`, `patch`). See [hooks.md](hooks.md) for the v1
  hook surface and the deferred-work section that captures how `run`
  and its approval store slot back in later. The PRD remains the
  aspirational target; `run` is explicitly a v1.x feature.
- **`elu audit` and `elu policy` are deferred to v1.x.** Both depend
  on the capability-approval model that ships with `run`; without
  `run` there is nothing meaningful for them to gate. Their CLI stubs
  remain in place so the surface is reserved.
- **Multi-platform package support (OCI-style image index)** stays
  deferred per the PRD.

Everything else in the PRD is in scope for v1: store, layers,
manifest, resolver, importers, outputs, registry (client **and**
server, minimal), CLI package-manager workflow (`install`, `add`,
`remove`, `lock`, `update`, `stack`), authoring workflow.

> **Scope ≠ status.** The list above is what v1 *targets*. Tracking
> for what is currently *implemented* lives in `cx` under `WKIW`.
> The resolver-driven CLI surface — multi-ref `install`/`stack`,
> lockfile lifecycle, and the `add`/`remove`/`lock`/`update` verbs —
> is tracked as children of `WKIW.wX0h`.

---

## Workspace layout

elu is a single Cargo workspace. Each ring in the PRD is its own
crate. Crate boundaries enforce the ring model at build time — a
lower ring cannot accidentally depend on a higher one, because the
dependency would be rejected by Cargo.

```
elu/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── elu-store/              # ring 1: CAS + hash types + refs + GC
│   ├── elu-layers/              # ring 2: tar + zstd/gzip + whiteouts + per-layer apply
│   ├── elu-manifest/           # ring 3: Manifest struct + TOML/JSON + validation
│   ├── elu-hooks/              # hook op interpreter (declarative ops, v1)
│   ├── elu-resolver/           # ring 4: ref → manifest-hash, dep graph, flatten
│   ├── elu-importers/          # ring 5: apt/npm/pip adapters (future rings)
│   ├── elu-outputs/            # ring 6: dir/tar/qcow2 materializers
│   ├── elu-registry/           # ring 7: HTTP client + server (async)
│   ├── elu-stacker/            # orchestration: Resolution → unpack → run hooks
│   └── elu-cli/                # ring 8: clap entry point, binary target `elu`
├── docs/
│   ├── prd/                    # product requirements (what)
│   └── design/                 # implementation design (how, this directory)
└── src/                        # retired; the top-level `src/main.rs` is
                                # replaced by `crates/elu-cli/src/main.rs`
```

### Hash types and the crate graph

The PRD's ring model has the store at ring 1 and manifest at ring 3.
But hash types (`Hash`, `DiffId`, `BlobId`, `ManifestHash`) need to be
visible to everyone. The design choice: **these types live in
`elu-store`, and higher rings depend on `elu-store` for types even if
they don't touch the filesystem.** This matches the PRD's mental
model — ids are store concepts — and avoids a dedicated `elu-core`
types crate whose only job is sharing four newtypes.

Dependency direction (cargo edges, source of truth: `cargo metadata`):

```
elu-store                                  (no elu deps; ring 1)
   ↑
elu-layers   elu-manifest                  (ring 2 / 3; siblings, no
   ↑              ↑                         edge between them. Layers
   |              |                         is pure tar primitives —
   |              |                         no hook or resolver knowledge.)
   |              |
   |              ↓
   |          elu-hooks            ← elu-manifest, elu-store
   |              ↑
   |          elu-resolver         ← elu-manifest, elu-store
   |              ↑
   |          elu-registry         ← elu-manifest, elu-store  (async boundary)
   |              ↑
   |          elu-outputs          ← elu-manifest
   |              ↑
   └────────→ elu-stacker          ← elu-layers, elu-hooks, elu-resolver,
                  ↑                                          elu-manifest, elu-store
              elu-importers        ← elu-manifest, elu-store
                  ↑
              elu-cli              ← everything
```

`elu-stacker` is the orchestration crate that owns the "apply a
Resolution into a directory and run its post-unpack hook" flow. It
sits above layers + hooks + resolver because it composes all three;
prior to the WKIW.0CZW cleanup this orchestration lived inside
`elu-layers`, which inverted the PRD ring model. Now `elu-layers` is
strictly tar primitives.

The resolver depends on `elu-manifest` (it walks dependency trees)
and on `elu-store` (it pins and fetches). `elu-importers` sits above
resolver/outputs because it produces packages by calling into all of
them. `elu-registry` is the only crate using tokio; everything below
it is sync.

---

## Async boundary

**Sync core, async at the network edge — except `elu-resolver` is
async-capable.** Store, layers, manifest, hooks, outputs, importers,
and stacker are fully sync. `elu-registry` and `elu-resolver` are the
two async-flavored crates.

`elu-registry` is async by construction: HTTP client (reqwest) and
server (axum), tokio runtime. Parallel blob fetches are the reason —
serial fetches would dominate any resolver that needs more than a
couple of packages from the network.

`elu-resolver` exposes an async `VersionSource` trait whose methods
return `impl Future<…>`. The body of `resolve` is `pub async fn`. A
sync caller (`elu lock` against an `OfflineSource`) reaches the
resolver through a `tokio::runtime::Builder::new_current_thread`
block_on, paying nothing for the async coloring at runtime; an
async caller (registry-backed source from `elu install`) reuses the
same trait without bridging. The earlier "everything below registry
is sync" statement was aspirational — keeping the resolver sync
forces a sync→async hop at every fetched manifest, which is awkward
when most of the resolver's I/O is the source's parallel network
calls. Living with the async coloring at this one ring is cheaper.

`elu-cli` starts a tokio runtime lazily: only commands that touch the
network (`install`, `publish`, `pull`, `fetch`, `serve`) enter the
runtime. Commands like `elu build`, `elu gc`, `elu stack` run with no
runtime at all.

Concretely, from the CLI:

```rust
// elu-cli/src/main.rs
fn main() -> ExitCode {
    match parse_command() {
        Cmd::Build(args)   => run_sync(build::run(args)),
        Cmd::Install(args) => run_async(install::run(args)),
        // ...
    }
}

fn run_async<F: Future<Output = Result<()>>>(f: F) -> ExitCode { /* tokio */ }
fn run_sync<T>(r: Result<T>) -> ExitCode { /* no runtime */ }
```

The sync → async transition is one-way: sync code never awaits, async
code calls into sync code via `tokio::task::spawn_blocking` when it
hits a sync boundary (e.g. the resolver needs to walk the local store
while fetching from the registry).

---

## Error strategy

- **Libraries use `thiserror`.** Each crate defines its own `Error`
  enum with variants specific to that layer. Library errors never use
  `anyhow::Error`; they produce typed errors that callers can match
  on.
- **The CLI uses `anyhow`.** `elu-cli` converts typed library errors
  into `anyhow::Error` for printing and into stable error codes for
  `--json` output. The `--json` error envelope (see
  [cli.md](cli.md) when written) is populated by a single
  `impl From<&SomeError> for JsonErrorCode` per library crate.
- **No `unwrap()` in library code** outside of `#[cfg(test)]` and
  genuinely-infallible operations (e.g. `Mutex::lock()` on an
  uncontended mutex where poisoning is a bug).
- **Errors carry context.** An error from `store.put_blob` that fails
  at the rename step includes the blob_id and the tmp path. Error
  messages are for humans; error codes are for machines.

Error codes are stable across versions once shipped. A code is
retired by being marked deprecated and leaving it unused; its number
is never reassigned.

---

## Platform support

Following OCI's posture:

- **Tier 1: Linux (x86_64, aarch64).** Everything works. CI runs the
  full test suite.
- **Tier 1: macOS (aarch64).** Everything except Linux-specific hook
  ops works. Authoring, building, publishing, resolving, stacking,
  and running the registry all work. When `run` is added post-v1,
  kernel confinement on macOS is its own design decision.
- **Tier 2: Windows (x86_64).** Portable parts work: store, manifest,
  resolver, registry client. File permission bits round-trip through
  tar but are advisory (NTFS doesn't have Unix mode bits in the same
  way); hook ops that set modes succeed but the resulting files may
  not honor the exact bits. Windows is best-effort — the CI matrix
  includes it for basic smoke tests, not for the full suite.

Path handling uses `camino::Utf8Path` throughout — elu paths are
always UTF-8, both because tar requires it in the `ustar` header and
because registries exchange paths as JSON strings. Non-UTF-8 paths
from the local filesystem are rejected at the `elu build` boundary
with a clear error.

Multi-platform packages (OCI image index equivalent) are deferred per
the PRD.

---

## Rust toolchain

- **Edition: 2024.**
- **MSRV: latest stable at the time of writing**, tracked in the
  workspace `Cargo.toml`. We do not support old rustcs; elu is
  infrastructure, not a library shipped to third parties, and the
  cost of MSRV policy outweighs the benefit.
- **Lints: `clippy::all`, `clippy::pedantic` (with targeted
  allows).** Warnings are errors in CI.
- **Formatting: `rustfmt` with defaults.** No config drift.

---

## Boring defaults

The following crate choices are recorded here once so individual
design docs can reference them without re-justifying. Each is picked
because it is the obvious, widely-used, well-maintained choice for
the job.

| Job | Crate | Why |
|---|---|---|
| Hashing (sha256) | `sha2` | RustCrypto, pure Rust, streaming API. |
| TOML parsing | `toml` | The reference crate, serde-integrated. |
| JSON parsing | `serde_json` | Same. Used for canonical manifest serialization. |
| Serde derives | `serde` | Ubiquitous. |
| Semver | `semver` | The reference crate; matches Cargo's semantics. |
| Tar | `tar` | Mature, streaming, handles PAX headers correctly. |
| Zstd | `zstd` | Official bindings. |
| Gzip | `flate2` | The standard choice. |
| CLI parsing | `clap` (derive) | Standard. `--json` support via a wrapper layer. |
| Glob matching | `globset` | Fast, well-tested, gitignore-compatible semantics. |
| Path types | `camino` | Always-UTF-8 paths. Avoids `OsStr` noise. |
| Temp files | `tempfile` | Standard. Used for `tmp/` staging in the store. |
| File locks | `fs2` | Simple advisory locks via `flock`/`LockFileEx`. |
| Typed errors | `thiserror` | Library crates. |
| Untyped errors | `anyhow` | CLI crate only. |
| HTTP client | `reqwest` (async, rustls) | Only used in `elu-registry`. |
| HTTP server | `axum` | Tokio-native, layered middleware. |
| SQL | `rusqlite` | Sync. Used by the registry server. Not by anything else. |
| Async runtime | `tokio` (multi-thread) | Only in `elu-registry` and on the CLI edge. |
| Unified diff (patch op) | `diffy` | Pure Rust, applies unified diffs. |
| qcow2 output | shell-out to `qemu-img` | No reasonable pure-Rust alternative in 2026. |

Deviation from this table is fine if a component design doc states
the reason. No design doc should introduce a new crate without
writing the one-sentence rationale.

---

## State store

Every piece of persistent state elu owns is either (a) a file under
the store root (`$XDG_DATA_HOME/elu`), (b) a file under the config
root (`$XDG_CONFIG_HOME/elu`), or (c) a SQLite database owned by the
registry server. There is no hidden state. A full inventory:

| State | Location | Format | Owner |
|---|---|---|---|
| Blob objects | `<store>/objects/<algo>/<ab>/<cd...>` | raw bytes | `elu-store` |
| diff_id → blob_id index | `<store>/diffs/<algo>/<ab>/<cd...>` | one-line text | `elu-store` |
| Manifest cache index | `<store>/manifests/` | filenames = blob_ids | `elu-store` |
| Local refs | `<store>/refs/<ns>/<name>/<version>` | one-line text | `elu-store` |
| GC lock | `<store>/locks/gc.lock` | empty file (`flock`) | `elu-store` |
| Tmp staging | `<store>/tmp/<random>` | scratch | `elu-store` |
| Per-user policy | `<config>/policy.toml` | TOML | `elu-hooks` (future `run`) |
| Project policy | `./.elu/policy.toml` | TOML | `elu-hooks` (future `run`) |
| Lockfile | `./elu.lock` | TOML | `elu-resolver` |
| Source manifest | `./elu.toml` | TOML | `elu-manifest` |
| Registry DB | `<server>/registry.sqlite` | SQLite | `elu-registry` (server only) |
| Registry blob store | `<server>/blobs/...` | CAS layout | `elu-registry` (server only) |

Approval state (for the future `run` capability model) will be flat
TOML files at `<config>/approvals/<manifest-hash>.toml`, plus an
append-only `<config>/approvals.log.jsonl` audit log. Design reserved
in [hooks.md](hooks.md#deferred-run-and-capability-approvals); no
code is written for this in v1.

SQLite exists only inside the registry server binary. Clients do not
run SQLite; they hit the HTTP API. Any design doc that wants to
introduce SQLite outside `elu-registry` needs a conversation first.

---

## Testing

- **Unit tests** live alongside the code in each crate under
  `#[cfg(test)]` modules.
- **Integration tests** live in each crate's `tests/` directory and
  exercise the public API.
- **End-to-end tests** live in `elu-cli/tests/` and drive the binary
  against a temporary store and a locally-spawned registry server.
  The e2e suite is the acceptance gate: if a feature isn't covered
  there, it isn't shipped.

Per global project workflow, features are built TDD in vertical
slices: one slice writes a failing test, greens it, commits; the
next slice does the same. Design docs are written once up front so
the slices can be executed against a stable target; they are not
revised mid-slice unless a real discovery invalidates them.

---

## Out of scope

Recorded here so they don't quietly creep in:

- **Plugin systems.** Not for ops, not for outputs, not for
  importers. See the PRD — adding a plugin boundary is a tax on every
  future change.
- **A build system.** elu packages files that already exist.
- **A container runtime.** elu produces filesystem trees, not
  processes.
- **OCI tooling bridges.** elu is byte-compatible with OCI at the
  layer level, but v1 ships neither an OCI importer nor an OCI
  exporter. Both are cleanly additive.
- **Multi-platform manifests.** OCI image-index equivalent. Deferred
  per PRD.
- **The `run` hook op and capability-approval model.** Deferred per
  this document's "Scope for v1" section.

---

## Open questions

None that block the first implementation slice. Component design
docs may raise their own; those propagate here if they affect more
than one crate.
