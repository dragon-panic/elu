# elu-hooks: Post-Unpack Op Interpreter

Implementation design for the declarative op interpreter described
in [../prd/hooks.md](../prd/hooks.md). This crate executes the
ordered `[[hook.op]]` list in a manifest against a staging
directory after the stacker has merged all layers.

---

## v1 scope and the deferred surface

**v1 ships only declarative ops.** The `run` op and the entire
capability-approval model (policy files, `ask`/`safe`/`trust`
modes, approval lockfile entries, manifest-hash-keyed approvals,
landlock kernel confinement, the diff UX on upgrade) are **deferred
to v1.x**. The PRD describes the full target; this design doc
implements v1 and reserves the shape the deferred pieces will take.

Why defer: the sandbox is the largest single piece of engineering
in the whole project (landlock, seccomp, namespaces, and their
equivalents on macOS/Windows), and the rest of elu is useful
without it. Shipping the declarative ops first gets us a working
package manager; shipping the capability model second gets us the
trust story that differentiates it from apt/npm/pip. The order is a
staging choice, not an abandonment.

Concretely, in v1:

- `HookOp::Run { .. }` does not exist as an enum variant. A manifest
  that tries to declare `type = "run"` is **rejected** at parse
  time by `elu-manifest` with a clear "run op is reserved for
  future versions" error.
- There is no policy file, no approval prompt, no lockfile approval
  entries. The lockfile has no `[package.hook_approval]` table in
  v1.
- The default hook mode is `safe` — declarative ops run, everything
  else is an error. A CLI `--hooks=off` flag still exists, because
  a consumer might want to stack a package without any hook work.
- `elu inspect <hash>` renders the op list for a manifest; there is
  no approval prompt behind it.

See the [Deferred work](#deferred-work-run-and-capability-approvals)
section below for the shape the v1.x work will take, so the v1
code leaves the right extension points.

---

## The op interpreter

```rust
// crates/elu-hooks/src/lib.rs
use camino::{Utf8Path, Utf8PathBuf};
use elu_manifest::{Manifest, HookOp};

pub struct HookRunner<'a> {
    staging: &'a Utf8Path,
    package: &'a PackageContext<'a>,
    mode: HookMode,
}

#[derive(Clone, Debug)]
pub struct PackageContext<'a> {
    pub namespace: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub kind: &'a str,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HookMode {
    /// Run every op in the manifest. v1 default.
    Safe,
    /// Run no ops at all. The stacker's output is whatever the
    /// layer tars produced; nothing else happens. Useful for
    /// inspection flows.
    Off,
    // Ask, Trust — reserved for v1.x with `run` support.
}

impl<'a> HookRunner<'a> {
    pub fn new(
        staging: &'a Utf8Path,
        package: &'a PackageContext<'a>,
        mode: HookMode,
    ) -> Self;

    /// Execute ops against the staging directory in order.
    /// On any error, the caller is responsible for discarding the
    /// staging directory — this function does not roll back.
    pub fn run(&self, ops: &[HookOp]) -> Result<HookStats, HookError>;
}

#[derive(Debug, Default)]
pub struct HookStats {
    pub ops_run: usize,
    pub files_changed: u64,
}
```

The interpreter dispatches on `HookOp` and calls a per-variant
handler. Each handler takes the staging path (as a root) and the
op's fields. None of them spawn a subprocess; none of them touch
anything outside staging.

```rust
pub fn run(&self, ops: &[HookOp]) -> Result<HookStats, HookError> {
    if matches!(self.mode, HookMode::Off) {
        return Ok(HookStats::default());
    }
    let mut stats = HookStats::default();
    for (i, op) in ops.iter().enumerate() {
        let r = match op {
            HookOp::Chmod   { paths, mode }           => self.do_chmod(paths, mode),
            HookOp::Mkdir   { path, mode, parents }   => self.do_mkdir(path, mode.as_deref(), *parents),
            HookOp::Symlink { from, to, replace }     => self.do_symlink(from, to, *replace),
            HookOp::Write   { path, content, mode, replace }
                                                      => self.do_write(path, content, mode.as_deref(), *replace),
            HookOp::Template { input, output, vars, mode }
                                                      => self.do_template(input, output, vars, mode.as_deref()),
            HookOp::Copy    { from, to }              => self.do_copy(from, to),
            HookOp::Move    { from, to }              => self.do_move(from, to),
            HookOp::Delete  { paths }                 => self.do_delete(paths),
            HookOp::Index   { root, output, format }  => self.do_index(root, output, *format),
            HookOp::Patch   { file, source, fuzz }    => self.do_patch(file, source, *fuzz),
        };
        r.map_err(|e| HookError::Op { index: i, source: Box::new(e) })?;
        stats.ops_run += 1;
    }
    Ok(stats)
}
```

Ops are executed in declaration order. On failure, the entire hook
run aborts and returns an error; the caller (the stacker or an
output format) is responsible for discarding the staging directory.
There is no per-op rollback — partial progress is fine *because*
the staging directory is discarded on failure. Atomic staging-to-
output rename (see `elu-outputs`) is the commit point for the
whole stack-plus-hook sequence.

---

## Path safety

Every op resolves its path arguments through a single helper that
enforces the staging-root invariant:

```rust
// crates/elu-hooks/src/path.rs

/// Resolve `rel` against `staging`, rejecting any path that
/// escapes `staging` after normalization. Also rejects absolute
/// paths and NUL-containing paths. Returns the canonical path
/// inside staging.
pub fn resolve_in_staging(
    staging: &Utf8Path,
    rel: &str,
) -> Result<Utf8PathBuf, HookError>;
```

Rules:

1. `rel` must be a relative path. Absolute paths (`/...`, `\...`,
   Windows drive prefixes) are rejected.
2. After `..` normalization, the result must still be under
   `staging`. Attempts to escape (`../../etc/passwd`) are rejected.
3. Symlinks within staging are followed for *reading*, but before a
   write we verify the final destination is still under staging
   via a realpath check. This closes the stacker-covered
   TOCTOU-free case: if an earlier op replaced a subdir with a
   symlink to `/etc`, a later op writing inside that subdir is
   rejected.
4. NUL bytes are rejected.

Glob patterns (used by `chmod`, `delete`) expand via `globset`
rooted at staging and return only matches that pass
`resolve_in_staging`. A pattern that matches no files is a no-op by
default; `strict` mode (future) would make it an error.

---

## Per-op implementations

Each op is a single small function. Sketches:

### `chmod`

```rust
fn do_chmod(&self, paths: &[String], mode: &str) -> Result<(), HookError> {
    let parsed = ModeSpec::parse(mode)?;   // "+x" or "0755"
    for pat in paths {
        let matches = glob_in_staging(self.staging, pat)?;
        for path in matches {
            let cur = std::fs::metadata(&path)?.permissions().mode();
            let new = parsed.apply(cur);
            std::fs::set_permissions(&path, Permissions::from_mode(new))?;
        }
    }
    Ok(())
}
```

`ModeSpec` parses either a symbolic mode (`+x`, `u+rw,g-w`) or an
octal mode (`0755`). Symbolic parsing is a small state machine; no
crate buys enough to justify the dep.

On Windows, mode bits are advisory — `set_permissions` maps the
write bit; execute and group bits are no-ops. The op still
succeeds; the resulting files simply don't have the exact Unix
permissions the manifest declared.

### `mkdir`

```rust
fn do_mkdir(&self, path: &str, mode: Option<&str>, parents: bool) -> Result<(), HookError> {
    let dest = resolve_in_staging(self.staging, path)?;
    if parents {
        std::fs::create_dir_all(&dest)?;
    } else {
        match std::fs::create_dir(&dest) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => { /* no-op */ }
            Err(e) => return Err(e.into()),
        }
    }
    if let Some(m) = mode {
        let parsed = ModeSpec::parse(m)?;
        let cur = std::fs::metadata(&dest)?.permissions().mode();
        let new = parsed.apply(cur);
        std::fs::set_permissions(&dest, Permissions::from_mode(new))?;
    }
    Ok(())
}
```

### `symlink`

```rust
fn do_symlink(&self, from: &str, to: &str, replace: bool) -> Result<(), HookError> {
    let link = resolve_in_staging(self.staging, from)?;
    // `to` is NOT resolved — symlink targets are relative-to-link
    // or absolute-at-runtime (per PRD).
    if link.exists() {
        if !replace { return Err(HookError::SymlinkExists(link)); }
        std::fs::remove_file(&link)?;
    }
    #[cfg(unix)]   std::os::unix::fs::symlink(to, &link)?;
    #[cfg(windows)] std::os::windows::fs::symlink_file(to, &link)?;
    Ok(())
}
```

### `write`

```rust
fn do_write(&self, path: &str, content: &str, mode: Option<&str>, replace: bool)
    -> Result<(), HookError>
{
    let dest = resolve_in_staging(self.staging, path)?;
    if dest.exists() && !replace {
        return Err(HookError::FileExists(dest));
    }
    let interp = interpolate(content, self.package, &BTreeMap::new())?;
    atomic_write(&dest, interp.as_bytes())?;
    if let Some(m) = mode {
        std::fs::set_permissions(&dest, Permissions::from_mode(ModeSpec::parse(m)?.apply(0o644)))?;
    }
    Ok(())
}
```

`atomic_write` writes to `<dest>.tmp.<rand>` then renames, matching
the store's discipline.

### `template`

```rust
fn do_template(&self, input: &str, output: &str,
               vars: &BTreeMap<String, String>, mode: Option<&str>)
    -> Result<(), HookError>
{
    let src  = resolve_in_staging(self.staging, input)?;
    let dest = resolve_in_staging(self.staging, output)?;
    let tpl  = std::fs::read_to_string(&src)?;
    let out  = interpolate(&tpl, self.package, vars)?;
    atomic_write(&dest, out.as_bytes())?;
    // mode defaults to the source mode; see PRD
    let mode = match mode {
        Some(m) => ModeSpec::parse(m)?.apply(0o644),
        None    => std::fs::metadata(&src)?.permissions().mode(),
    };
    std::fs::set_permissions(&dest, Permissions::from_mode(mode))?;
    Ok(())
}
```

### Interpolation

```rust
// crates/elu-hooks/src/interpolate.rs

/// Substitute {package.*} and {var.*} in `src`. Unknown references
/// are a hard error — no silent fallthrough.
pub fn interpolate(
    src: &str,
    pkg: &PackageContext,
    vars: &BTreeMap<String, String>,
) -> Result<String, HookError>;
```

Supported references (per PRD):

| Pattern | Source |
|---|---|
| `{package.namespace}` | `pkg.namespace` |
| `{package.name}` | `pkg.name` |
| `{package.version}` | `pkg.version` |
| `{package.kind}` | `pkg.kind` |
| `{var.<name>}` | `vars.get(name)` |

Implementation: a tiny state machine that scans for `{`, reads
until the matching `}`, looks up the key, and substitutes. We do
**not** use a templating crate (`tera`, `handlebars`, etc.) —
those introduce their own logic (`{% if %}`, filters, loops) that
would expand the surface area of what a hook can do. Simple
`{name}` substitution is the whole feature; anything more is a
publisher shipping a templating engine they didn't intend to ship.

Unknown references are hard errors. `{env.HOME}` is not supported
and produces `HookError::UnknownInterpolation`.

### `copy`, `move`

```rust
fn do_copy(&self, from: &str, to: &str) -> Result<(), HookError> {
    let matches = glob_in_staging(self.staging, from)?;
    let dest_base = resolve_in_staging(self.staging, to)?;
    for src in matches {
        let dest = if dest_base.ends_with('/') || dest_base.is_dir() {
            dest_base.join(src.file_name().unwrap())
        } else {
            dest_base.clone()
        };
        std::fs::copy(&src, &dest)?;
    }
    Ok(())
}

fn do_move(&self, from: &str, to: &str) -> Result<(), HookError> {
    // As copy, then remove the source. `rename` is the optimization
    // when source and dest live on the same filesystem, which inside
    // a single staging dir is always true.
    let matches = glob_in_staging(self.staging, from)?;
    let dest_base = resolve_in_staging(self.staging, to)?;
    for src in matches {
        let dest = /* as above */;
        std::fs::rename(&src, &dest)?;
    }
    Ok(())
}
```

### `delete`

```rust
fn do_delete(&self, paths: &[String]) -> Result<(), HookError> {
    for pat in paths {
        for path in glob_in_staging(self.staging, pat)? {
            if path.is_dir() { std::fs::remove_dir_all(&path)?; }
            else              { std::fs::remove_file(&path)?;    }
        }
    }
    Ok(())
}
```

Recursive deletion is scoped to staging by the path resolver —
there is no way for a malicious `paths = ["../../"]` to touch
anything outside.

### `index`

```rust
fn do_index(&self, root: &str, output: &str, format: IndexFormat)
    -> Result<(), HookError>
{
    let root  = resolve_in_staging(self.staging, root)?;
    let dest  = resolve_in_staging(self.staging, output)?;
    let mut entries: Vec<(Utf8PathBuf, Hash)> = Vec::new();
    for entry in walkdir::WalkDir::new(&root).sort_by_file_name() {
        let e = entry?;
        if e.file_type().is_file() {
            let mut h = elu_store::Hasher::new();
            let mut f = std::fs::File::open(e.path())?;
            std::io::copy(&mut f, &mut HashWriter(&mut h))?;
            entries.push((Utf8PathBuf::from_path_buf(e.into_path()).unwrap(), h.finalize()));
        }
    }
    let bytes = match format {
        IndexFormat::Sha256List => render_sha256_list(&entries),
        IndexFormat::Json       => serde_json::to_vec_pretty(&entries)?,
        IndexFormat::Toml       => toml::to_string(&entries)?.into_bytes(),
    };
    atomic_write(&dest, &bytes)?;
    Ok(())
}
```

Deterministic ordering (`sort_by_file_name`) means two hook runs
over the same tree produce byte-identical indices.

### `patch`

```rust
fn do_patch(&self, file: &str, source: &PatchSource, fuzz: bool)
    -> Result<(), HookError>
{
    let target = resolve_in_staging(self.staging, file)?;
    let diff_text = match source {
        PatchSource::Inline { diff } => diff.clone(),
        PatchSource::File   { from } => {
            std::fs::read_to_string(resolve_in_staging(self.staging, from)?)?
        }
    };
    let original = std::fs::read_to_string(&target)?;
    let patch = diffy::Patch::from_str(&diff_text)?;
    let patched = diffy::apply(&original, &patch)
        .map_err(|_| HookError::PatchFailed(target.clone()))?;
    atomic_write(&target, patched.as_bytes())?;
    Ok(())
}
```

`diffy` handles unified diffs in pure Rust. `fuzz` is not supported
by diffy in v1; if a package sets `fuzz = true` and the patch
doesn't apply cleanly, we fail with a clear error. Fuzzy matching
is a future enhancement if real packages need it.

---

## Errors

```rust
#[derive(thiserror::Error, Debug)]
pub enum HookError {
    #[error("op {index} failed: {source}")]
    Op { index: usize, source: Box<HookError> },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid mode: {0}")]
    InvalidMode(String),

    #[error("path escapes staging: {0}")]
    PathEscape(String),

    #[error("glob: {0}")]
    Glob(String),

    #[error("symlink already exists: {0}")]
    SymlinkExists(Utf8PathBuf),

    #[error("file already exists: {0}")]
    FileExists(Utf8PathBuf),

    #[error("patch failed: {0}")]
    PatchFailed(Utf8PathBuf),

    #[error("unknown interpolation: {0}")]
    UnknownInterpolation(String),

    #[error("diffy: {0}")]
    Diffy(#[from] diffy::ParsePatchError),

    #[error("hook mode is 'off'")]
    HookModeOff,  // reserved for callers that insist a hook must run
}
```

Error codes: `hooks.op_failed`, `hooks.io`, `hooks.invalid_mode`,
`hooks.path_escape`, `hooks.glob`, `hooks.symlink_exists`,
`hooks.file_exists`, `hooks.patch_failed`,
`hooks.unknown_interpolation`, `hooks.diffy`,
`hooks.mode_off`.

---

## Deferred: `run` and capability approvals

The full capability model described in the PRD lands in v1.x. This
section records the extension points the v1 code leaves open so the
v1.x work is additive, not a rewrite.

- `HookOp` gains a new variant:
  ```rust
  Run {
      command: Vec<String>,
      reads:   Vec<String>,
      writes:  Vec<String>,
      network: bool,
      timeout_ms: Option<u64>,
      env:     BTreeMap<String, String>,
  }
  ```
  Adding the variant is an additive serde change. Existing
  manifests without `run` ops continue to parse.

- `HookMode` gains `Ask` and `Trust` variants. The dispatch
  function adds a pre-check that consults the approval store
  before executing a `run` op.

- An **`ApprovalStore` trait** is introduced, owned by
  `elu-hooks`. It maps `ManifestHash → ApprovalRecord`. The v1.x
  implementation is flat TOML files at
  `$XDG_CONFIG_HOME/elu/approvals/<manifest-hash>.toml` plus an
  append-only `approvals.log.jsonl` audit log. See
  [overview.md](overview.md#state-store).
  ```rust
  pub trait ApprovalStore {
      fn get(&self, hash: &ManifestHash) -> Option<ApprovalRecord>;
      fn put(&self, hash: &ManifestHash, record: &ApprovalRecord) -> Result<(), ApprovalError>;
      fn remove(&self, hash: &ManifestHash) -> Result<(), ApprovalError>;
      fn list(&self) -> Result<Vec<ApprovalRecord>, ApprovalError>;
  }
  ```

- A **`Sandbox` trait** is introduced, also owned by `elu-hooks`.
  The v1.x Linux implementation uses landlock + seccompiler +
  namespaces (via `rustix`). macOS and Windows implementations are
  deferred further.
  ```rust
  pub trait Sandbox {
      fn exec(
          &self,
          command: &[String],
          staging: &Utf8Path,
          reads: &[Glob],
          writes: &[Glob],
          network: bool,
          timeout: Duration,
          env: &BTreeMap<String, String>,
      ) -> Result<ExitStatus, SandboxError>;
  }
  ```

- A **`HookPolicy`** type is introduced, parsing
  `~/.config/elu/policy.toml` and `./.elu/policy.toml` per PRD.
  v1 has no policy type because there are no policy-gated ops.

- **Lockfile approval entries**: `elu.lock` gains a
  `[package.hook_approval]` table per PRD.

- **Diff UX**: the CLI gains an approval prompt renderer that diffs
  the previously-approved capability profile against the new one.
  This is a pure formatting concern; no new elu-hooks API.

None of these trait definitions exist in the v1 code. The v1 code
leaves a single extension point: the `HookOp` enum is
`#[non_exhaustive]` so we can add the `Run` variant without
requiring downstream code that matches on `HookOp` to re-declare
all existing variants. That's the only structural accommodation we
make in v1 for the deferred work.

---

## Non-goals (v1)

- **No subprocess execution.** No `run`, no shell, no fork.
- **No kernel confinement.** No landlock, no seccomp, no
  namespaces. These land with `run`.
- **No per-layer hooks.** Per PRD, hooks are per-package; per-layer
  is an additive future change.
- **No guest-side hooks.** Staging is host-side.
- **No templating engine.** `{package.*}` and `{var.*}`
  substitution only.

---

## Testing strategy

- **Unit tests**: `ModeSpec::parse`, `resolve_in_staging` over
  adversarial inputs, `interpolate` over every supported and
  unsupported reference.
- **Integration tests** (per op): build a temp staging dir, call
  `HookRunner::run` with a constructed `HookOp`, assert the
  filesystem state afterward. Every op gets at least a happy-path
  test and an error-path test.
- **Path safety tests**: for every op, construct an op whose paths
  contain `..`, absolute paths, and symlink traversal attempts;
  assert `HookError::PathEscape`.
- **Determinism tests**: for `index`, run the op twice against the
  same tree and assert byte-identical output.
- **End-to-end**: via `elu-cli`, build a package with every op
  declared, install it, verify the materialized output.
