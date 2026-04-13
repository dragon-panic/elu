# Output Formats

An output is the materialization of a stack into a usable artifact. The
resolver produces a pinned layer list; the stacker assembles it into a
staging directory; the output format takes the staging directory and
turns it into something the user can *do* something with — a tarball
to ship, a directory to chroot into, a qcow2 image to boot.

Output formats are the final step of every elu operation that produces
something concrete. They share the resolver, the store, and the
stacker. Each output owns only the last mile: "given a finalized
staging tree, produce target X."

---

## Contract

Every output format implements:

```
output.materialize(staging_dir, target_path, options) -> result
```

By the time an output is called:

- Every layer has been applied into `staging_dir`.
- The post-unpack hook (if any) has run successfully.
- `staging_dir` is a plain host directory the output can read.

By the time the output returns:

- `target_path` contains the finalized artifact.
- The staging directory has been cleaned up.
- Any output-specific metadata (compression info, checksums, size)
  is in the returned `result`.

An output is a pure transformer from "directory" to "artifact." It
does not resolve, does not stack, does not mutate the store. This is
what makes adding a new output cheap: it touches only one component.

---

## `dir`

The simplest output. Rename the staging directory into place.

```
elu stack ox-community/postgres-query -o ./out --format dir
```

Semantics:

- `target_path` must not exist, or `--force` must be set.
- The staging directory is `mv`'d to `target_path` atomically when
  both are on the same filesystem. If they are not, contents are
  copied and the source is removed.
- The result directory has the permissions, ownership, and mtimes
  encoded in the layer tar entries, subject to the user's umask and
  the filesystem's capabilities.

Options:

| Option | Effect |
|--------|--------|
| `--force` | Remove an existing `target_path` before materializing. |
| `--owner` | Rewrite ownership to `uid:gid` on all files. |
| `--mode` | Apply an additional mode mask (e.g. `go-w`). |

`dir` is the default for interactive use: `elu stack foo -o ./foo` is
expected to Just Work.

---

## `tar`

Pack the staging directory as a tar archive.

```
elu stack ox-community/postgres-query -o skill.tar --format tar
elu stack ox-community/postgres-query -o skill.tar.zst --format tar --compress zstd
```

Semantics:

- The tar entries are written in sorted path order so that the
  output is byte-reproducible given the same inputs.
- Ownership, mode, and mtimes are copied from the staging tree.
- Compression is optional and is applied as a streaming transform
  on the output bytes (`gzip`, `zstd`, `xz`, `none`). The
  compression format is **not** part of the layer model — it is a
  transport detail at the output stage.
- Symlinks are preserved.

Options:

| Option | Effect |
|--------|--------|
| `--compress` | `none` (default), `gzip`, `zstd`, `xz`. |
| `--level` | Compression level, format-specific. |
| `--deterministic` | Force mtimes to epoch 0, uids/gids to 0. Default: on. |

`tar` is what you produce to ship a stack somewhere that does not
have elu installed. A consumer on the other end can extract it with
standard tools.

---

## `qcow2`

Produce a QEMU-compatible disk image from the staging directory plus
an operating system base.

This is the most complex output because it is the only one that
requires something other than "the staging tree." A bootable VM image
needs a kernel, an init system, a bootloader, and a partition table.
elu does not build those itself — it assembles them from a declared
**base image layer**, then overlays the staging tree on top.

```
elu stack ox-runner-image -o runner.qcow2 --format qcow2 \
    --base debian/bookworm-minbase
```

### Base images

A base image is itself an elu package — typically produced by the apt
importer running `debootstrap`-like logic against a minimal package
set, or imported from an existing OCI image. A base image package has:

- `kind = "os-base"`
- Metadata declaring the expected architecture, kernel package, and
  init system
- One or more layers containing the root filesystem

The `qcow2` output loads the base image package first, stacks its
layers into the qcow2 root, then stacks the user's layers on top. The
result is a bootable image where the user's content sits over a
standard OS.

### Finalization

Some OS-level finalization has to happen inside the guest (running
`update-initramfs`, generating `/etc/machine-id`, regenerating
caches). The qcow2 output provides a `guest-finalize` hook declared
in the base image's metadata:

```toml
[metadata.os-base]
arch     = "amd64"
kernel   = "linux-image-amd64"
init     = "systemd"
finalize = ["update-initramfs", "-u"]
```

The output runs the finalize command inside the guest via a short-
lived qemu invocation (or chroot, if the host supports it safely). This
is the **only** place elu runs anything inside a guest, and it does so
only when a base image explicitly opts in.

This is not the same thing as the manifest's `[hook]` (see
[layers.md](layers.md)). Those are host-side, per-package, and
universal. Guest finalization is output-specific and base-image-
specific. They do not overlap.

Options:

| Option | Effect |
|--------|--------|
| `--base` | The base image package reference. Required. |
| `--size` | Target disk size. Defaults to fit + 20%. |
| `--format-version` | qcow2 format version, defaults to 3. |
| `--no-finalize` | Skip guest finalization. Image may not boot. |

### When qcow2 makes sense

The consumer that cares most about qcow2 is seguro — it boots these
images as sandboxed VMs for agent runners. See [seguro.md](seguro.md).
Other consumers can use qcow2 too; elu does not special-case seguro.

A user who only wants a container-compatible filesystem should use
`tar` or `dir` and feed the result to a container runtime. qcow2 is
for "I need an actual bootable VM."

---

## Adding a New Output

Output formats are closed in v1: `dir`, `tar`, `qcow2`. No plugin
boundary. Adding a new output is a code change in elu. The contract
is small enough that this is cheap:

1. Implement `materialize(staging_dir, target_path, options)`.
2. Register the format name.
3. Document the options here.

Candidates for future outputs:

| Format | Notes |
|--------|-------|
| `overlay` | Extract each layer to its own directory and expose the stack as a kernel overlayfs mount (stacked lowerdirs, optional writable upperdir). The zero-copy-shared path for consumers that want many concurrent read-only views of the same content. Replaces the reflink/hardlink unpack strategies we explicitly rejected in [layers.md](layers.md). |
| `oci` | Produce an OCI image layout. Enables push to container registries. |
| `squashfs` | Produce a read-only squashfs image. Useful for initrd-style uses. |
| `raw` | Raw disk image. Simpler than qcow2, harder to ship. |
| `iso` | Bootable ISO. Installers, live images. |

None of these are in v1. They are listed here so the shape of the
extension point is visible.

---

## Interface Sketch

```
outputs.list() -> list of format names

outputs.materialize(format, staging_dir, target_path, options) -> result

# Per-format convenience wrappers, same contract
outputs.dir.materialize(staging_dir, target_path, options)
outputs.tar.materialize(staging_dir, target_path, options)
outputs.qcow2.materialize(staging_dir, target_path, options)
```

CLI exposes these via `--format`:

```
elu stack <refs> -o <path> [--format dir|tar|qcow2] [format-specific options]
```

The default format is inferred from the target path:
`foo/` or existing directory → `dir`; `*.tar`, `*.tar.gz`, `*.tar.zst`
→ `tar`; `*.qcow2` → `qcow2`. Explicit `--format` always wins.

---

## Non-Goals

**No output that mutates the store.** Outputs only read from staging.
If an output needs to produce an auxiliary artifact (a checksum file,
a manifest of contents), it writes it alongside the target, not back
into the store.

**No streaming outputs from partial stacks.** An output runs only
after the full stack is assembled. Partial or incremental output
(e.g. "stream the tar as each layer is unpacked") is not worth the
complexity in v1.

**No in-place updates.** `elu stack foo -o ./foo` on an existing
target either refuses or (with `--force`) replaces wholesale. There
is no "update this directory with the new layers." Consumers that
want that behavior build it on top — stack to a new path, swap the
symlink, remove the old path.

**No signing of outputs.** If a consumer needs a signature on a tar
or qcow2, they sign it with their own tooling after elu produces it.
elu owns content integrity in the store via hashing; signatures are
a trust concern above it.
