//! Guest finalization for qcow2 output. Runs the base manifest's
//! `finalize` command inside the image.
//!
//! v1 finalize strategy (per proposal open question 2):
//! - Prefer `fuse2fs` + `chroot` on same-arch hosts (no root required).
//! - Fall back to `qemu-system-<arch>` when fuse2fs is missing or
//!   architecture differs from the host.
//!
//! When neither path is available, surface [`OutputError::External`] with
//! enough detail to guide the operator. Use `--no-finalize` to skip.

use camino::Utf8Path;

use super::{OsBase, which};
use crate::error::OutputError;

/// Run `base.finalize` against the raw ext4 image at `raw`.
pub fn run(_raw: &Utf8Path, base: &OsBase) -> Result<(), OutputError> {
    if base.finalize.is_empty() {
        return Ok(());
    }
    // fuse2fs + chroot path.
    if which("fuse2fs").is_some() && host_arch_matches(&base.arch) {
        return Err(OutputError::External(
            "guest finalize via fuse2fs not yet wired — rerun with --no-finalize".into(),
        ));
    }
    // qemu path.
    let qemu = format!("qemu-system-{}", base.arch);
    if which(&qemu).is_some() {
        return Err(OutputError::External(format!(
            "guest finalize via {qemu} not yet wired — rerun with --no-finalize"
        )));
    }
    Err(OutputError::External(format!(
        "no finalize path available (need fuse2fs or {qemu}); rerun with --no-finalize"
    )))
}

fn host_arch_matches(arch: &str) -> bool {
    let host = std::env::consts::ARCH;
    matches!(
        (host, arch),
        ("x86_64", "amd64" | "x86_64")
            | ("aarch64", "aarch64" | "arm64")
            | ("x86", "i386" | "x86")
            | ("arm", "arm" | "armhf")
    )
}
