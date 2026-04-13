# Hooks: Capability Model for Post-Unpack Finalization

A hook is a sequence of operations a package declares in its manifest,
to be executed against the staging directory after all layers have
been applied and before any output is finalized. Hooks exist for
finalization that cannot be captured in the layer tar itself — things
like `chmod +x bin/*`, generating a combined index file, or
substituting `{package.version}` into a template.

Every other package ecosystem has a hook mechanism of this kind, and
in every other package ecosystem the hook is "run this shell command
with the privileges of the install process." The result has been,
predictably, that install hooks are the primary supply chain attack
vector across apt, npm, pip, cargo, gem, and every other ecosystem
that followed the same pattern.

elu takes a different position: **the operations a package can
perform at install time are a closed set that elu itself
implements, and anything outside that set is named `run`, marked
dangerous, and consented to separately.** There is no shell, no
plugin boundary, no way for a publisher to introduce a new
operation by shipping a clever manifest. The publisher supplies
*arguments*; elu supplies *behavior*.

This distinction is load-bearing and worth stating plainly:

- **Native ops** (`chmod`, `mkdir`, `symlink`, `write`, `template`,
  `copy`, `move`, `delete`, `index`, `patch`, and whatever else
  elu grows over time) are functions in elu's own code. The
  `write` op's entire behavior is "write these bytes to this
  path." There is no code path inside `write` that reads
  `~/.ssh` or opens a socket, so it cannot. This is real
  enforcement, on every platform, with zero kernel help, because
  the enforcement mechanism is simply that *elu did not write a
  function that does anything else*. A native op is as
  trustworthy as elu itself.
- **`run`** is the escape hatch. When a manifest uses `run`, elu
  is `execve`-ing a binary the package supplied. The declared
  `reads`/`writes`/`network` are **disclosure**, not guarantees.
  elu makes no promises about what the binary actually does. Every
  consent prompt, every approval diff, every `elu audit` listing
  marks `run` ops visibly — they are qualitatively different from
  native ops and the user should know.

**The product goal is to grow the native op set until `run` is
rare.** Every time a publisher reaches for `run` to do something
mundane — `systemctl daemon-reload`, `update-alternatives`,
`fc-cache` — that is a signal that the native set is missing an op,
and the right fix is usually *add a native op for that*, not *make
`run` nicer*. The importer story is the scoreboard: every apt
`postinst` script a future importer can map to native ops is a win;
every one that has to be rewrapped as `run` is a loss, documented.
If 80%+ of real packages never touch `run`, then the supply chain
is 80%+ made of operations whose behavior is elu's own code —
which is a trust property no other package manager comes close to.

elu is **not a sandbox.** It is a package manager. It can run
inside a sandbox (seguro, a container, a CI runner with its own
limits), and the declared capabilities on `run` ops can optionally
be handed to a kernel-level mechanism like landlock where one is
available — but elu itself does not contain, jail, or confine
anything. Its security value comes from two things: (1) native ops
are enforced by virtue of being elu's own code, and (2) `run` is
disclosed up front, keyed to consent, and re-prompts on any change.
Anything beyond that is the consumer's layer.

---

## Expected to Iterate

The specific op set described below is a v1 best guess. The 90%
case — chmod, symlink, generated indices, template substitution —
is well-understood from looking at what existing package
ecosystems' install scripts actually do. What we don't know yet is:

- Which long-tail cases we're missing, and will therefore force
  publishers to reach for `run` when a declarative op could have
  served.
- Which op field shapes turn out to be awkward in practice.
- Whether the default `run` capability declarations (`reads`,
  `writes`, `network`, `timeout_ms`) are the right axes, or
  whether real use will call for more (CPU time, memory, specific
  syscalls, access to `/dev/*`, etc.).
- Whether the policy file format's glob/allow/deny structure
  handles the shapes of trust decisions users actually want to
  express.
- Whether the CLI verbs (`inspect`, `audit`, `policy`) carve the
  space the way users will look for them, or whether the
  ergonomics want different names or different groupings.

**These will iterate.** v1 ships the op set and surface described
here. v1.x adds ops based on what packages actually need and
refines field shapes based on what publishers and operators find
awkward. A future schema bump can tighten op semantics if anything
turns out to be wrong. None of these iterations are breaking for
existing packages as long as the change is additive (new op types,
new optional fields); semantics changes are gated behind a
manifest schema version bump and elu rejects manifests with
schema versions it does not understand.

**The closed-set property is not negotiable.** Iteration means
elu-side code changes, reviewed and shipped in elu like any other
feature. It does not mean a plugin boundary. Every op, now and
forever, is implemented in elu's own code. The set of ops that
exists will grow; the mechanism by which ops are defined will
not.

The sections below describe v1. Read them as "this is the
starting point and we'll see what breaks," not as "this is the
final answer."

---

## The Op Set

A manifest's `[[hook.op]]` entries are executed in declaration order
against the staging directory. Each op has a `type` field selecting
the operation and additional fields specific to that op. All paths
are **relative to the staging directory**. Absolute paths are
rejected. Paths containing `..` segments that would escape the
staging root are rejected. Symlinks within staging are followed, but
a symlink whose target escapes staging is treated as a reference to
a non-existent file and the op fails.

The op set is **closed**. elu implements each op in its own code.
Adding a new op is an elu change — not a package change, not a
plugin, not a script. This is the property that makes the declarative
side of the hook model trustworthy: a publisher cannot introduce new
capabilities by shipping a clever manifest.

### `chmod`

Change the mode of one or more files or directories.

```toml
[[hook.op]]
type  = "chmod"
paths = ["bin/*", "scripts/setup.sh"]
mode  = "+x"
```

| Field | Required | Meaning |
|-------|----------|---------|
| `paths` | yes | List of glob patterns rooted at staging. |
| `mode`  | yes | Either a symbolic mode (`+x`, `u+rw,g-w`) or octal (`0755`). |

### `mkdir`

Create a directory, optionally with a specific mode. Creating a
directory that already exists is a no-op unless `mode` is specified,
in which case the existing directory's mode is updated.

```toml
[[hook.op]]
type = "mkdir"
path = "var/cache"
mode = "0755"
```

| Field | Required | Meaning |
|-------|----------|---------|
| `path`   | yes | Directory path rooted at staging. |
| `mode`   | no  | Octal mode. Default `0755`. |
| `parents` | no | Bool; create parent dirs if missing. Default `false`. |

### `symlink`

Create a symbolic link within staging. The target may be relative
(to the link's parent) or absolute; an absolute target is allowed
but it refers to the path *as seen at runtime in the materialized
output*, not to anywhere on the host.

```toml
[[hook.op]]
type = "symlink"
from = "bin/latest"
to   = "bin/v1.2"
```

| Field | Required | Meaning |
|-------|----------|---------|
| `from` | yes | The symlink path to create (rooted at staging). |
| `to`   | yes | The symlink target. |

If `from` already exists, the op fails unless `replace = true` is
set.

### `write`

Create a file with literal or interpolated content.

```toml
[[hook.op]]
type    = "write"
path    = "etc/version"
content = "{package.version}\n"
mode    = "0644"
```

| Field | Required | Meaning |
|-------|----------|---------|
| `path`    | yes | File path rooted at staging. |
| `content` | yes | String content. Interpolation is scoped; see below. |
| `mode`    | no  | Octal mode. Default `0644`. |
| `replace` | no  | Bool; overwrite an existing file. Default `false`. |

### `template`

Like `write`, but reads a template file from staging, substitutes
interpolation variables, and writes the result to a target path. The
template file must be present in a layer; this op does not create
templates, it consumes them.

```toml
[[hook.op]]
type   = "template"
input  = "etc/config.toml.in"
output = "etc/config.toml"
vars   = { listen_port = "8080" }
```

| Field | Required | Meaning |
|-------|----------|---------|
| `input`  | yes | Template source path rooted at staging. |
| `output` | yes | Output path rooted at staging. |
| `vars`   | no  | A table of additional substitution variables. |
| `mode`   | no  | Octal mode on the output. Default: the mode of `input`. |

Interpolation uses the same `{name}` syntax throughout elu. Available
namespaces in hook ops:

| Pattern | Source |
|---------|--------|
| `{package.namespace}` | The package's `namespace`. |
| `{package.name}` | The package's `name`. |
| `{package.version}` | The package's `version`. |
| `{package.kind}` | The package's `kind`. |
| `{var.name}` | Any `vars` declared on this op. |

Interpolation does **not** include environment variables, hostnames,
timestamps, or anything external. The only inputs to the template
are manifest-declared.

### `copy` and `move`

Relocate files within staging. Source and destination globs must
both be rooted in staging.

```toml
[[hook.op]]
type = "copy"
from = "share/examples/*.conf"
to   = "etc/examples/"

[[hook.op]]
type = "move"
from = "obsolete/old-name.txt"
to   = "new-name.txt"
```

### `delete`

Remove files or directories within staging. Recursive.

```toml
[[hook.op]]
type  = "delete"
paths = ["tmp/", "*.bak", "share/doc/legacy/**"]
```

### `index`

Walk a subtree, hash every file, and write a manifest file listing
paths and hashes. Used to generate integrity or discovery indices
for consumers that want them.

```toml
[[hook.op]]
type   = "index"
root   = "bin"
output = "bin/.index"
format = "sha256-list"
```

| Field | Required | Meaning |
|-------|----------|---------|
| `root`   | yes | Directory to index, rooted at staging. |
| `output` | yes | Output file path. |
| `format` | no  | `sha256-list` (default), `json`, `toml`. |

### `patch`

Apply a unified diff to an existing file. The diff is declared
inline or referenced from a file in the staging tree.

```toml
[[hook.op]]
type  = "patch"
file  = "etc/defaults.conf"
diff  = """
@@ -3 +3 @@
-debug = false
+debug = true
"""
```

| Field | Required | Meaning |
|-------|----------|---------|
| `file` | yes | Target file rooted at staging. |
| `diff` | exactly one of | Inline unified diff. |
| `from` | exactly one of | Path to a file in staging containing the diff. |
| `fuzz` | no | Bool; allow fuzzy matching. Default `false`. |

A patch that does not apply cleanly fails the hook.

---

## The `run` Escape Hatch

The ten declarative ops above cover the vast majority of
finalization needs. For the remainder — running a format-specific
tool, invoking a compiled helper, calling out to a system binary
that is already present — there is `run`.

`run` is the **only** op in elu that executes a binary outside
elu itself. It is the dangerous capability and is treated as such
throughout the design: declared permissions are mandatory, consumer
policy controls whether it executes, the default policy does not
trust it, and upgrades to packages that use it trigger re-approval.

```toml
[[hook.op]]
type     = "run"
command  = ["objcopy", "--strip-debug", "lib/libfoo.so"]
reads    = ["lib/**"]
writes   = ["lib/**"]
network  = false
timeout_ms = 30000
```

| Field | Required | Meaning |
|-------|----------|---------|
| `command` | yes | Argv list. **Not** a shell string. No interpolation. |
| `reads`   | yes | Glob patterns the command is allowed to read. Rooted at staging. |
| `writes`  | yes | Glob patterns the command is allowed to write. Rooted at staging. |
| `network` | yes | Bool. Whether the command may make network calls. |
| `timeout_ms` | no | Wall-clock timeout. Default 30_000. Max 300_000. |
| `env`     | no  | Map of environment variables. Keys restricted; see below. |

### Why `command` is argv, not shell

Shell parsing is where command injection lives. `command = ["ls",
"{user_input}"]` is safe; `command = "ls {user_input}"` is a
vulnerability waiting for a user_input that contains `;`. elu does
not accept shell strings in `run`. Period. If a publisher needs
shell features (pipes, redirects, globbing), they bake those into a
script they ship in a layer, and `command` invokes the script
directly: `command = ["sh", "scripts/finalize.sh"]`. The script's
contents are then part of the manifest's content hash and visible
to auditors.

### `reads` and `writes`

These are glob patterns describing the filesystem reach the
command is allowed to have. They are **always rooted in the
staging directory** — no absolute paths, no `..`, no access to
anywhere else on the host.

By **default** (see [Optional Kernel Confinement for `run`
](#optional-kernel-confinement-for-run) below), these declarations
are recorded, hashed into the manifest, surfaced in approval
prompts, and inspected by policy — but not enforced at the kernel
level. An honest publisher's command reads and writes only what
they declared; a determined malicious publisher can lie. Inspection
and audit tools warn when declarations look suspicious (a command
that reads `**` and writes `**` is effectively unconstrained), and
the lie is auditable after the fact.

When the consumer **opts in to landlock on Linux**, `reads`/`writes`
are enforced at the kernel level: the process is confined to the
declared globs and attempts to read or write outside them fail with
`EACCES`. Equivalent opt-in mechanisms on macOS and Windows are
future work.

### `network`

A bool. If `false` (the default behavior for any hook that doesn't
think carefully), the command is declared to make no network calls.
By default this is disclosure, not enforcement: an honest command
honors it, a lying command may try anyway, and `elu audit` can flag
the discrepancy after the fact. With opt-in landlock on Linux, the
command runs in a network namespace with no routes, so network
calls fail outright.

Any package declaring `network = true` is surfaced prominently by
`elu inspect` and `elu audit` — it is the single most load-bearing
capability in the model and consumers should look at it closely.

### `env`

Optional map of environment variables set on the command. Keys are
restricted to a small allowlist of known-safe vars: `HOME`,
`PATH`, `LANG`, `LC_*`, `TMPDIR`, and any variable with the prefix
`ELU_`. Arbitrary keys are rejected. `PATH` is reconstructed by elu
to point only at staging-local paths and any system paths the
policy allows; it is not passed through from the parent
environment. This prevents a package from exfiltrating via
environment variable the caller might have set.

Interpolation: same `{package.*}` namespace as elsewhere. No
interpolation of `{env.*}`.

---

## Policy Model

Policy decides whether an op (declarative or `run`) is allowed to
execute. Policy is the user's, not the publisher's: a manifest
declares what a package *wants* to do; policy decides whether the
user *consents*.

Policies live in two places:

1. **User policy** at `$XDG_CONFIG_HOME/elu/policy.toml`. Applies
   to every elu invocation by that user.
2. **Project policy** at `.elu/policy.toml` in a project directory.
   Overrides user policy with stricter rules (never looser).

Command-line flags override both for a single invocation:

```
elu install ... --hooks=safe     # declarative ops only
elu install ... --hooks=ask      # prompt on run (default)
elu install ... --hooks=trust    # honor all run ops, no prompt
elu install ... --hooks=off      # refuse any hook at all
```

### Default mode

The default is **`ask`**. Safe is too strict for a working user
(packages break and people disable the system to get unblocked);
trust is too permissive for the long tail (attacks land). `ask`
gets people thinking about trust at the moment of first install,
and approvals are persistent, so they only have to think once per
(package, version) pair.

**`trust` is never the default.** Not for a new install, not for a
convenience flag, not on any platform, not for any reason. This is
the single most important rule in this document.

### Policy file format

```toml
# ~/.config/elu/policy.toml

[hooks]
default = "ask"                # ask | safe | trust | off

# Declarative ops: almost always allowed. Consumers can pare this
# down, but the default is that declarative ops run.
[hooks.declarative]
chmod    = true
mkdir    = true
symlink  = true
write    = true
template = true
copy     = true
move     = true
delete   = true
index    = true
patch    = true

# Allow rules for run ops. Each rule is an AND of its fields:
# publisher/namespace AND command pattern AND path patterns AND network.
[[hooks.allow]]
namespace = "debian/*"
run       = ["ldconfig", "update-*"]
reads     = ["**"]
writes    = ["var/lib/dpkg/**", "etc/ld.so.cache"]
network   = false

[[hooks.allow]]
publisher = "ox-community"
run       = ["objcopy --strip-debug *"]
reads     = ["lib/**"]
writes    = ["lib/**"]
network   = false

# Deny rules override allow rules.
[[hooks.deny]]
publisher = "sketchy-corp"
```

### Glob syntax

All patterns use standard glob syntax, the same as gitignore and
Claude Code's tool approval language:

| Pattern | Matches |
|---------|---------|
| `*` | Anything within a single path segment (or command argument). |
| `**` | Anything across path segments. |
| `?` | A single character. |
| `[abc]` | One of `a`, `b`, `c`. |

Command patterns match against the command's argv joined with
single spaces. `run = ["git commit *"]` matches `["git", "commit",
"-m", "message"]` but not `["git", "push"]`.

Publisher and namespace patterns use the same glob syntax against
the `namespace/name` identifier. `namespace = "debian/*"` matches
any Debian-imported package; `publisher = "ox-*"` matches any
publisher whose id starts with `ox-`.

---

## Version Pinning: Approvals Are Keyed on Manifest Hash

This is the part that makes the model actually protect against the
attack it's designed to prevent.

**Approvals are keyed on the manifest hash, not on
`namespace/name@version`.**

Because the manifest hash transitively commits to every layer
diff_id, every dependency, every `[[hook.op]]` entry, and every
field of `[metadata]`, any change to what a package does changes
its manifest hash. An approval for `b3:8f7a...` does not cover
`b3:3b9e...`, even if both claim to be version `1.0.1` of the same
package.

This closes the central attack: *publisher ships 1.0.0 with benign
ops, user approves, publisher ships 1.0.1 with a malicious run op,
approval silently carries over.* In elu, the approval does not
carry over, because the approval key is the hash and the hash has
changed. The user is prompted again, and the prompt shows the
diff.

### Lockfile integration

Approvals live in the lockfile alongside the pinned hashes:

```toml
# elu.lock

schema = 1

[[package]]
namespace = "ox-community"
name      = "postgres-query"
version   = "0.3.2"
hash      = "b3:8f7a1c2e4d..."

[package.hook_approval]
approved_at = "2026-04-12T10:30:00Z"
approved_by = "dragon@laptop"
ops_summary = ["chmod(bin/*)", "run(ldconfig)"]
run         = ["ldconfig"]
reads       = ["lib/**"]
writes      = ["lib/**", "var/ld.so.cache"]
network     = false
```

The lockfile is committed to version control. A CI run with
`elu install --locked` uses exactly these approvals and fails if
any installed package's manifest hash does not have a matching
`hook_approval` entry in the lockfile. There is no way for a CI
run to "just install" a new package's hooks without the
corresponding approval having been committed by a human.

### The diff UX

When `elu install` or `elu update` encounters a package whose
manifest hash is not already approved in the lockfile, it
presents the user with an approval prompt. Crucially, on an
**upgrade** (a previously-approved package whose hash is
changing), the prompt shows the *diff* between the old capability
profile and the new one:

```
┌─ Hook approval required ──────────────────────────────────────┐
│                                                                │
│ ox-community/postgres-query                                    │
│   0.3.2 → 0.3.3                                                │
│   prior approval: b3:8f7a... (approved 2026-04-12 by dragon)  │
│   new manifest:   b3:3b9e...                                   │
│                                                                │
│ Declarative ops:                                               │
│   ✓ chmod(bin/*)        unchanged                              │
│                                                                │
│ Run ops:                                                       │
│   ✓ run(ldconfig)       unchanged                              │
│   + run(curl *)         NEW                                    │
│                                                                │
│ Capabilities:                                                  │
│     reads:   lib/**                                            │
│   + reads:   etc/** (NEW)                                      │
│     writes:  lib/**                                            │
│   - network: false                                             │
│   + network: true (NEW — package will make network calls)     │
│                                                                │
│ [a]pprove   [r]efuse   [i]nspect   [d]iff manifest             │
└────────────────────────────────────────────────────────────────┘
```

A clean patch release (everything unchanged) is a one-keystroke
approval. A patch release that quietly added a `run(curl *)` op
and flipped `network` to true is a moment of real attention. The
attacker's choice becomes: either declare the new capabilities in
the manifest and hope the user clicks through without reading, or
confine themselves to whatever capabilities the previous version
was already allowed. Both options are dramatically worse for the
attacker than the status quo in apt/npm/pip, where the upgrade
silently inherits trust.

### First-time approval

A package with no prior approval (a fresh install, not an upgrade)
prompts with the full capability profile rather than a diff:

```
┌─ Hook approval required ──────────────────────────────────────┐
│                                                                │
│ ox-community/postgres-query @ 0.3.2                            │
│   manifest: b3:8f7a1c2e4d...                                   │
│   publisher: ox-community (verified)                           │
│                                                                │
│ This package declares the following hook operations:          │
│   • chmod bin/*                                                │
│   • run ldconfig                                               │
│     reads:   lib/**                                            │
│     writes:  lib/**, var/ld.so.cache                           │
│     network: false                                             │
│                                                                │
│ [a]pprove   [r]efuse   [i]nspect                               │
└────────────────────────────────────────────────────────────────┘
```

### Refusing approval

Refusing approval fails the install for that package. The rest of
the resolution continues as long as the refused package isn't a
dependency of something else that is proceeding. If it is, the
whole install fails: there is no partial-state install where some
packages are present and others were skipped because the user said
no to their hooks.

---

## Shape-Based Consent: The Claude Code Bash Model

Manifest-hash pinning is strict. For a working developer that
upgrades dependencies frequently, re-approving every patch release
manually is friction — *for the cases where the upgrade actually
changed something dangerous*. For inert packages (no hooks at all)
and for packages whose native ops are byte-identical across
versions, the manifest-hash rule is already free: the hooks
section hashes to the same value, so prior consent still applies
and there is no prompt. The friction lives entirely in the `run`
case, which is precisely where strictness is most load-bearing.

The escape valve is borrowed directly from Claude Code's bash
permission model: a user can opt in, in their own policy file, to
**shape-based consent** for `run` ops from a specific publisher
within a specific version range and a specific capability envelope.
A new version's `run` ops auto-approve if and only if every field
matches the declared shape; anything outside the shape still
prompts.

```toml
# ~/.config/elu/policy.toml

[[hooks.allow]]
publisher     = "ox-community"
name          = "postgres-query"
version_range = "^1.0"

# Shape envelope. An upgrade auto-approves if its run ops fit
# inside this shape. An upgrade that exceeds it still prompts.
run           = ["ldconfig", "update-*"]   # argv-glob patterns
reads         = ["lib/**", "etc/**"]
writes        = ["lib/**"]
network       = false
```

The argv-glob patterns work the same way Claude Code's `Bash(git
log:*)` patterns do: each `run` op's argv is joined with single
spaces and matched against the patterns in the rule. `run =
["ldconfig", "update-*"]` matches `["ldconfig"]` and `["update-ca-
certificates", "--fresh"]`, but does **not** match `["curl",
"https://evil"]`. A version that introduces a new `run(curl *)`
op falls outside the envelope and prompts. A version that flips
`network` to true falls outside the envelope and prompts. A
version that widens `writes` to `**` falls outside the envelope
and prompts.

This earns back the ergonomics of "I just trust ox-community's
1.x releases for this set of operations" without opening the door
to "I just trust ox-community's 1.x releases to do whatever." The
envelope is declared explicitly by the user, in the user's own
config, and the publisher cannot loosen it. The version number is
not a trust input; the *shape* is.

**`run` stays marked dangerous in every UI surface, regardless of
how smooth the consent flow is.** Shape-based consent is an
ergonomic affordance for power users who have already decided the
risk is acceptable. It does not promote `run` ops to the same trust
class as native ops, and `elu inspect`, `elu audit`, and the
approval prompts continue to flag `run` visibly. Ergonomics does
not launder risk.

Shape-based consent is **never the default.** It requires the user
to type the rule into their policy file. A fresh install on a new
machine uses `ask` mode with strict manifest-hash keying, and the
strict mode is the right place to live for almost everyone almost
all the time — because if the native op set is doing its job, the
strict mode has nothing to ask about for the overwhelming majority
of packages.

---

## Optional Kernel Confinement for `run`

Native ops do not need this section. They are enforced by virtue
of being elu's own code on every platform — `write` only writes,
`chmod` only chmods, `symlink` only symlinks, all bounded to the
staging directory by the op argument types themselves. There is no
kernel mechanism involved and none needed. A native op is as
trustworthy as elu itself, and that trust property holds equally
on Linux, macOS, Windows, BSD, or anywhere else elu compiles.

This section is about `run`, and only `run`. When a manifest uses
`run`, elu is invoking a binary it does not control, and the
declared `reads`/`writes`/`network` are disclosure rather than
guarantees. For consumers who want those declarations to also be
*enforced*, elu can optionally hand them to a kernel-level
confinement mechanism on platforms that provide one. This is opt-in,
platform-specific, and explicitly **not** elu pretending to be a
sandbox. elu is a package manager. If you need a sandbox, run elu
inside seguro, a container, or whatever isolation your environment
provides; elu's confinement support is a convenience for the cases
where the OS makes it cheap to wire up.

| Mechanism | Platform | Status |
|-----------|----------|--------|
| None — declarations are disclosure only | all | **v1 default** |
| Landlock (filesystem) + user namespace (network) | Linux ≥5.13 | v1.x, opt-in |
| `sandbox-exec` profiles | macOS | future |
| AppContainer + Job Objects | Windows | future |

### Default: declared, not confined

By default, elu records the declared `reads`/`writes`/`network` on
every `run` op, presents them to the user during approval, commits
them to the manifest hash, and surfaces them in `elu inspect` and
`elu audit`. The command process itself runs with whatever
privileges the elu process has. If the binary lies — declares
`network = false` and then opens a socket — the lie is auditable
after the fact by observing what the command actually did, but it
is not prevented at the kernel level.

This is honest, and it is the right default for a package manager
that does not own the kernel. The protection comes from the
combination of: (1) the closed native op set means most packages
never use `run` at all, (2) using `run` is loud in every UI surface
so reviewers see it, (3) approvals are keyed on manifest hash so
any change to the `run` op forces re-prompting, and (4) the
declared capabilities give consumers and auditors a basis for
deciding whether to trust the package in the first place. A
determined attacker on the default tier can declare one thing and
do another, but they have to put the lie in the manifest, and the
manifest is hashed, signed (optionally), and visible.

### Opt-in landlock on Linux

When the consumer opts in (via policy or a CLI flag) and the
platform supports it, elu installs a landlock ruleset before
spawning each `run` command, permitting exactly the declared
`reads` and `writes` globs. If `network = false`, the process is
spawned in a new network namespace with no routes. The command
runs with the usual uid but cannot exceed its declared reach at
the kernel level. A binary that lies about its capabilities now
fails with `EACCES` instead of succeeding silently.

Landlock is present in Linux 5.13+. On older kernels, the
landlock path is unavailable and elu falls back to the default
(declared, not confined) with a warning.

This is a feature elu offers because the OS makes it cheap, not
because elu is in the business of sandboxing. The mental model is
"elu can hand the `run` declarations to the kernel for you on
Linux," not "elu confines packages."

### macOS and Windows

Future. `sandbox-exec` on macOS and AppContainer on Windows are
the analogous mechanisms, and the declared→platform-specific
mapping is real work that is not v1. Until then, the default
(declared, not confined) applies, and consumers who need
enforcement on those platforms should run elu inside a sandbox
their environment provides.

### Why this is not a contradiction

Saying "elu is not a sandbox" and "elu can install a landlock
ruleset before `run`" are not in tension. The first is about
what elu *is*: a package manager, whose security claims rest on
native ops being enforced by implementation. The second is about
what elu *can optionally do*: reuse the OS's own confinement
primitives for the one op where elu cannot enforce by
implementation. Native ops do not need confinement and do not get
it. `run` does need it and gets it where the OS supplies it. The
package manager remains a package manager either way.

---

## Inspect, Audit, and Policy Surface

Capability declarations only work if users can see them. Three CLI
surfaces are load-bearing:

### `elu inspect <ref>`

Shows the package's manifest with hook operations prominently
rendered. `run` ops are highlighted (ANSI red in terminal,
flagged in `--json`). Example:

```
$ elu inspect ox-community/postgres-query@0.3.2

  Package:    ox-community/postgres-query
  Version:    0.3.2
  Manifest:   b3:8f7a1c2e4d...
  Kind:       ox-skill
  Publisher:  ox-community (verified)

  Layers (2):
    b3:cc... (bin)    18432 bytes
    b3:dd... (docs)     512 bytes

  Hook operations:
    1. chmod "bin/*" +x
    2. run ["ldconfig"]
       reads:   lib/**
       writes:  lib/**, var/ld.so.cache
       network: false
       timeout: 30s
```

### `elu audit`

Scans a lockfile and reports packages whose capability profile
deserves review. Output is machine-readable (`--json`) and
human-readable (default). Checks include:

- Packages with `run` ops at all.
- Packages with `network = true`.
- Packages where `writes` extends beyond the package's own
  namespace convention (heuristic).
- Packages from unverified publishers.
- Packages whose approval in the lockfile does not match the
  current manifest (drift).
- Packages whose `run` command patterns contain `*` wildcards that
  match more than ~5 actual commands in a typical PATH.

`elu audit` is intended to be usable as a CI gate:

```
elu audit --fail-on network=true --fail-on unverified-publisher
```

Exit code non-zero means something in the lockfile trips a rule.

### `elu policy`

Manage policy. Show effective policy (merged user + project):

```
elu policy show
```

Test whether a specific package would be approved under the
current policy:

```
elu policy check ox-community/postgres-query@0.3.2
# prints: approved (rule: publisher=ox-community)
# or:    prompts (no matching allow rule)
# or:    denied (rule: publisher=sketchy-corp)
```

Add a new allow rule interactively:

```
elu policy allow --publisher ox-community \
    --run 'objcopy --strip-debug *' \
    --reads 'lib/**' --writes 'lib/**' \
    --network false
```

Remove an approval from the lockfile (e.g., to force re-prompting
on next install):

```
elu policy revoke ox-community/postgres-query
```

---

## Threat Model and What This Does Not Protect Against

The capability model is aimed at **supply chain attacks via hook
code**, which is historically the largest attack surface of
package ecosystems. It is not a general sandbox for everything a
package can do.

### What the model protects against

- **Malicious install scripts.** A package cannot execute arbitrary
  code at install time via the declarative ops, full stop. The op
  set does not include arbitrary code execution. Compromised
  packages in the majority case become harmless noise.
- **Silent trust carry-over on upgrades.** Manifest-hash approval
  keying means an upgrade that changes capabilities always
  prompts. The attacker can't sneak a new `run` op into a patch
  release without the user seeing the diff.
- **Declared-permission audit trail.** Even without kernel
  confinement, declared capabilities are visible in `elu inspect`
  and `elu audit`, giving humans and automation a basis for
  deciding whether to trust a package. An attacker using `run` has
  to either leave evidence in the declaration or confine themselves
  to previously-approved capabilities.
- **Cross-ecosystem typosquats via imported packages.** A
  `debian/curl` import that declares no `run` ops cannot execute
  postinst scripts; elu importers deliberately do not execute
  maintainer scripts (see [importers.md](importers.md)), and the
  resulting package is a pure file tree with no escape hatch.

### What the model does not protect against

- **Malicious content in layers.** Files that get unpacked into
  the staging directory can be anything the publisher wants.
  Staging contents then run under whatever trust the consumer
  gives them — if a consumer puts them on PATH or executes them,
  the files are trusted. elu's job ends at the staging directory;
  what consumers do with the files is the consumer's trust story.
- **Vulnerabilities in elu itself.** A buffer overflow in elu's
  tar reader is out of scope for this model. General software
  assurance applies to elu as a codebase like any other.
- **Determined attackers using `run` without kernel confinement.**
  By default, a `run` command whose declaration says
  `network = false` can still try to make network calls; the
  declaration is disclosure, not enforcement. Opt-in landlock on
  Linux closes this gap for `run`. On macOS and Windows, opt-in
  confinement is future work, and consumers who need enforcement
  there should run elu inside a sandbox their environment provides.
  None of this affects native ops, which are enforced by being
  elu's own code on every platform.
- **Social engineering of humans.** A user who hits `a` on every
  prompt without reading gets whatever the publisher asked for.
  The diff UX and the default-to-`ask` policy make the attacker's
  job harder, but they do not make it impossible. The threat
  model assumes users read the diff when it's non-trivial.

---

## Non-Goals

**No plugin system for new ops.** The op set is closed and
implemented in elu's code. A publisher cannot add a new op by
shipping a clever package. Adding a new op is an elu-side change,
reviewed and shipped like any other elu feature. This is the
property that makes declarative ops trustworthy in the first place.

**No Turing-complete scripting language in manifests.** No
conditionals beyond field presence, no loops, no arithmetic, no
expression evaluation beyond simple `{package.*}` interpolation.
If a publisher's finalization logic needs conditionals, they
implement it as a script in a layer and call it via `run`. The
script's contents become part of the manifest hash and are visible
to auditors.

**No per-layer hooks.** A hook is per-package and runs after all
layers are applied. Per-layer hooks are a reserved extension — if
a real use case appears, the schema grows a `[[layer.hook]]`
block; existing manifests continue to work unchanged. Not in v1.

**No hook for actions other than finalization.** There is no
"pre-unpack hook," no "on-uninstall hook," no "on-update hook."
The single hook runs once, at a known point, with a known
environment. Adding new hook slots is an invitation to confusion
and to attacks that exploit the differences between them. If a
consumer needs installation lifecycle events, the consumer
tracks them outside elu.

**No runtime hooks.** `[hook]` runs once, at stack time. It does
not run when the materialized output is used. A consumer that
wants something to happen at dispatch or boot time (a container
entrypoint, a systemd unit, a qcow2 init script) declares that
at the consumer layer, not in the elu manifest.

**No hook chaining across packages.** A package's hook cannot
invoke or observe another package's hook. Hooks are scoped to the
package that declares them, running against the package's
contribution to the staging tree only. Dependencies are
materialized before the depending package's layers and hook; the
depending package's hook sees the merged tree but cannot re-run
the dependency's hook.

**No signed-hooks-only mode.** A publisher signature over the
manifest covers the hook declaration transitively (it covers the
manifest hash, which covers the ops). But elu does not refuse
unsigned packages by default — that is a policy decision the
consumer makes. A cautious operator can set `hooks.default =
"safe"` to refuse all `run` ops regardless of signature.
