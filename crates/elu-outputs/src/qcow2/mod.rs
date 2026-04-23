//! qcow2 output: a QEMU-compatible disk image built from a user staging
//! tree overlaid onto an `os-base` package.
//!
//! Pipeline: merge base + user staging into one tree, build a raw ext4
//! image with `mke2fs -d`, optionally run the base's `finalize` command
//! inside the guest, convert the raw image to qcow2 with `qemu-img`.
//!
//! External binary dependencies are detected at the start of
//! `materialize` and surfaced as [`OutputError::External`] if missing.
//! See `docs/prd/outputs.md` § qcow2 for the contract.

pub mod base;
pub mod finalize;

use std::fs;
use std::path::Path;
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::OutputError;
use crate::{Outcome, Qcow2Opts};

pub use base::{OsBase, parse_os_base};

/// Materialize a qcow2 disk image at `target`.
///
/// `user_staging` is the caller's finalized staging tree. `base_staging`
/// is the finalized base-image tree (the caller resolves `--base` and
/// stages it separately). `base_meta` is the parsed `[metadata.os-base]`
/// block.
pub fn materialize(
    user_staging: &Utf8Path,
    base_staging: &Utf8Path,
    base_meta: &OsBase,
    target: &Utf8Path,
    opts: &Qcow2Opts,
) -> Result<Outcome, OutputError> {
    require_binary("mke2fs")?;
    require_binary("qemu-img")?;

    if !user_staging.as_std_path().is_dir() {
        return Err(OutputError::StagingNotDir(user_staging.to_path_buf()));
    }
    if !base_staging.as_std_path().is_dir() {
        return Err(OutputError::StagingNotDir(base_staging.to_path_buf()));
    }
    if target.as_std_path().exists() {
        if !opts.force {
            return Err(OutputError::TargetExists(target.to_path_buf()));
        }
        fs::remove_file(target.as_std_path()).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(OutputError::Io(e))
            }
        })?;
    }

    let workdir = tempfile::Builder::new()
        .prefix(".elu-qcow2.")
        .tempdir()
        .map_err(OutputError::Io)?;
    let workdir_path = Utf8PathBuf::from_path_buf(workdir.path().to_path_buf())
        .map_err(|p| OutputError::Io(std::io::Error::other(format!("non-utf8 path: {p:?}"))))?;

    // Stage 1: overlay user staging onto base staging into a merged tree.
    let merged = workdir_path.join("merged");
    merge_trees(base_staging, user_staging, &merged)?;

    // Stage 2: build raw ext4 image.
    let raw = workdir_path.join("disk.raw");
    let size = determine_size(&merged, opts.size)?;
    build_raw_ext4(&merged, &raw, size)?;

    // Stage 3: guest finalize (best-effort; skipped if requested or if
    // finalize is empty or fuse2fs/chroot are unavailable).
    if !opts.no_finalize && !base_meta.finalize.is_empty() {
        finalize::run(&raw, base_meta)?;
    }

    // Stage 4: convert raw → qcow2 via tmp+rename.
    let tmp = tmp_sibling(target);
    if let Some(parent) = target.parent()
        && !parent.as_str().is_empty()
    {
        fs::create_dir_all(parent.as_std_path())?;
    }
    convert_raw_to_qcow2(&raw, &tmp, opts.format_version)?;
    match fs::rename(tmp.as_std_path(), target.as_std_path()) {
        Ok(()) => {}
        Err(e) => {
            let _ = fs::remove_file(tmp.as_std_path());
            return Err(OutputError::Io(e));
        }
    }

    let bytes = fs::metadata(target.as_std_path())?.len();
    Ok(Outcome { bytes })
}

fn tmp_sibling(target: &Utf8Path) -> Utf8PathBuf {
    let name = target.file_name().unwrap_or("elu-out");
    let parent = target
        .parent()
        .filter(|p| !p.as_str().is_empty())
        .map(Utf8Path::to_path_buf)
        .unwrap_or_else(|| Utf8PathBuf::from("."));
    parent.join(format!(".{name}.tmp"))
}

pub fn require_binary(name: &str) -> Result<(), OutputError> {
    if which(name).is_some() {
        Ok(())
    } else {
        Err(OutputError::External(format!(
            "{name} not found on PATH; required for qcow2"
        )))
    }
}

pub fn which(name: &str) -> Option<Utf8PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Utf8PathBuf::from_path_buf(candidate).ok();
        }
    }
    None
}

/// Copy `base` into `dst`, then overlay `user` on top (files from `user`
/// win). `dst` must not exist.
fn merge_trees(base: &Utf8Path, user: &Utf8Path, dst: &Utf8Path) -> Result<(), OutputError> {
    fs::create_dir_all(dst.as_std_path())?;
    copy_tree(base.as_std_path(), dst.as_std_path())?;
    copy_tree(user.as_std_path(), dst.as_std_path())?;
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), OutputError> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let src_child = src.join(entry.file_name());
        let dst_child = dst.join(entry.file_name());
        if ft.is_symlink() {
            if dst_child.symlink_metadata().is_ok() {
                fs::remove_file(&dst_child).ok();
            }
            let link = fs::read_link(&src_child)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link, &dst_child)?;
        } else if ft.is_dir() {
            if !dst_child.is_dir() {
                fs::create_dir_all(&dst_child)?;
            }
            copy_tree(&src_child, &dst_child)?;
        } else {
            if dst_child.exists() {
                fs::remove_file(&dst_child).ok();
            }
            fs::copy(&src_child, &dst_child)?;
        }
    }
    Ok(())
}

fn determine_size(root: &Utf8Path, explicit: Option<u64>) -> Result<u64, OutputError> {
    if let Some(sz) = explicit {
        return Ok(sz);
    }
    let used = tree_bytes(root)?;
    // fit + 20%, minimum 16 MiB, rounded up to 1 MiB.
    let target = (used + used / 5).max(16 * 1024 * 1024);
    let mib = 1024 * 1024;
    Ok(target.div_ceil(mib) * mib)
}

fn tree_bytes(root: &Utf8Path) -> Result<u64, OutputError> {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir.as_std_path())? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|p| {
                OutputError::Io(std::io::Error::other(format!("non-utf8: {p:?}")))
            })?;
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

pub fn build_raw_ext4(
    src: &Utf8Path,
    raw: &Utf8Path,
    size_bytes: u64,
) -> Result<(), OutputError> {
    // Allocate the sparse backing file.
    let f = fs::File::create(raw.as_std_path())?;
    f.set_len(size_bytes)?;
    drop(f);

    // mke2fs -t ext4 -d <src> -L elu <raw>
    let status = Command::new("mke2fs")
        .args([
            "-t",
            "ext4",
            "-F",
            "-E",
            "nodiscard",
            "-L",
            "elu",
            "-d",
        ])
        .arg(src.as_std_path())
        .arg(raw.as_std_path())
        .status()
        .map_err(|e| OutputError::External(format!("mke2fs: {e}")))?;
    if !status.success() {
        return Err(OutputError::External(format!(
            "mke2fs exited with {status}"
        )));
    }
    Ok(())
}

pub fn convert_raw_to_qcow2(
    raw: &Utf8Path,
    out: &Utf8Path,
    version: u32,
) -> Result<(), OutputError> {
    let compat = match version {
        2 => "0.10",
        _ => "1.1",
    };
    let status = Command::new("qemu-img")
        .args(["convert", "-f", "raw", "-O", "qcow2", "-o"])
        .arg(format!("compat={compat}"))
        .arg(raw.as_std_path())
        .arg(out.as_std_path())
        .status()
        .map_err(|e| OutputError::External(format!("qemu-img: {e}")))?;
    if !status.success() {
        return Err(OutputError::External(format!(
            "qemu-img convert exited with {status}"
        )));
    }
    Ok(())
}
