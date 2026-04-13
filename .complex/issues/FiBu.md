# Hook capability model

The central trust story for elu. Package install hooks are a closed
set of declarative filesystem operations implemented in elu's own
code, plus a governed `run` escape hatch for executing binaries.
Capabilities must be declared up front, consumers grant them per
manifest hash (not per version), and upgrades that change a
package's capability profile force re-approval with a diff UX.

**Spec:** [`docs/prd/hooks.md`](../docs/prd/hooks.md)

## Key decisions (from PRD)

- **Ten declarative ops**: `chmod`, `mkdir`, `symlink`, `write`,
  `template`, `copy`, `move`, `delete`, `index`, `patch`. All
  implemented in elu's own code. No shell, no subprocess, no
  network. Paths rooted in staging; `..` and absolute paths
  rejected. Op set is closed — no plugin boundary.
- **One escape hatch**: `run` op with mandatory declared
  capabilities (`command` as argv not shell, `reads`/`writes` as
  staging-rooted globs, `network` bool, `env` allowlist,
  `timeout_ms`).
- **Policy model**: user policy at `$XDG_CONFIG_HOME/elu/policy.toml`,
  project override at `.elu/policy.toml`, `--hooks=` CLI flag.
  Glob syntax on commands/paths/publishers. Default is `ask`.
  **`trust` is never the default.**
- **Manifest-hash approval keying**: approvals live in `elu.lock`
  keyed on manifest hash. Any change to hook ops changes the hash
  and forces re-approval on upgrade. Diff UX highlights new run
  ops, widened reads/writes, network flips.
- **Opt-in semver-range envelope** for power users: auto-approves
  upgrades within a declared capability envelope and version range.
  Never a default; requires explicit policy file entry.
- **Enforcement tiers**: 0 (declared-only, all platforms, v1); 1
  (landlock + netns, Linux, v1.x opt-in); 2 (sandbox-exec, macOS,
  future); 3 (AppContainer, Windows, future).
- **Expected to iterate**: the op set, field shapes, policy
  format, and CLI verbs are v1 best guesses. The closed-set
  security property is not negotiable; iteration is elu-side code
  changes, not a plugin boundary.

## Acceptance

- Parse and validate `[[hook.op]]` entries in manifests.
- Implement all ten declarative ops with staging-rooted path
  enforcement.
- Implement `run` op with declared-capability recording.
- Implement policy file parsing (user + project merge, CLI
  override).
- Implement manifest-hash approval keying in `elu.lock`.
- Implement the approval prompt with diff UX for upgrades.
- Implement `elu inspect` op display, `elu audit`, `elu policy`.
- Landlock enforcement on Linux (v1.x, opt-in via policy).
- Semver-range envelope auto-approval respects envelope
  containment.
- `elu.lock --locked` refuses drift between committed approvals
  and current manifests.

## Dependencies

Blocked on:
- `LszD` (manifest format) — manifest must parse `[[hook.op]]`.
- `VJp1` (store) — approvals live alongside lockfile.

Blocks:
- `jfvm` (CLI) — needs `inspect`, `audit`, `policy`, `--hooks=`.
