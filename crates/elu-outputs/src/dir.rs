use std::fs;

use camino::Utf8Path;

use crate::error::OutputError;
use crate::{DirOpts, Outcome};

/// Materialize the `staging` tree at `target` as a plain directory.
///
/// Rename staging into place. The rename is the commit point — a failure
/// before it leaves `target` untouched.
pub fn materialize(
    staging: &Utf8Path,
    target: &Utf8Path,
    opts: &DirOpts,
) -> Result<Outcome, OutputError> {
    if !staging.as_std_path().is_dir() {
        return Err(OutputError::StagingNotDir(staging.to_path_buf()));
    }
    if target.as_std_path().exists() || is_dangling_symlink(target) {
        if !opts.force {
            return Err(OutputError::TargetExists(target.to_path_buf()));
        }
        remove_target(target)?;
    }
    if let Some(parent) = target.parent()
        && !parent.as_str().is_empty()
    {
        fs::create_dir_all(parent.as_std_path())?;
    }
    fs::rename(staging.as_std_path(), target.as_std_path())?;
    if opts.owner.is_some() || opts.mode_mask.is_some() {
        apply_owner_mode(target, opts.owner, opts.mode_mask)?;
    }
    Ok(Outcome {
        bytes: dir_size(target)?,
    })
}

#[cfg(unix)]
fn apply_owner_mode(
    target: &Utf8Path,
    owner: Option<(u32, u32)>,
    mode_mask: Option<u32>,
) -> Result<(), OutputError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, lchown};

    // Bottom-up: children before parents, so masking a directory's exec
    // bit doesn't prevent touching its children.
    let mut paths = walkdir(target)?;
    paths.reverse();
    paths.push(target.to_path_buf());
    for path in paths {
        let meta = fs::symlink_metadata(path.as_std_path())?;
        if let Some((uid, gid)) = owner {
            lchown(path.as_std_path(), Some(uid), Some(gid))?;
        }
        if let Some(mask) = mode_mask
            && !meta.file_type().is_symlink()
        {
            let new_mode = meta.mode() & mask;
            fs::set_permissions(path.as_std_path(), fs::Permissions::from_mode(new_mode))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_owner_mode(
    _target: &Utf8Path,
    _owner: Option<(u32, u32)>,
    _mode_mask: Option<u32>,
) -> Result<(), OutputError> {
    Err(OutputError::Unsupported(
        "--owner and --mode require a Unix host",
    ))
}

fn walkdir(root: &Utf8Path) -> Result<Vec<camino::Utf8PathBuf>, OutputError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir.as_std_path())? {
            let entry = entry?;
            let path = camino::Utf8PathBuf::from_path_buf(entry.path()).map_err(|p| {
                OutputError::Io(std::io::Error::other(format!("non-utf8 path: {p:?}")))
            })?;
            let ft = entry.file_type()?;
            if ft.is_dir() {
                stack.push(path.clone());
            }
            out.push(path);
        }
    }
    Ok(out)
}

fn is_dangling_symlink(path: &Utf8Path) -> bool {
    fs::symlink_metadata(path.as_std_path()).is_ok()
}

fn remove_target(target: &Utf8Path) -> Result<(), OutputError> {
    match fs::symlink_metadata(target.as_std_path()) {
        Ok(meta) => {
            if meta.is_dir() {
                fs::remove_dir_all(target.as_std_path())?;
            } else {
                fs::remove_file(target.as_std_path())?;
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(OutputError::Io(e)),
    }
}

fn dir_size(root: &Utf8Path) -> Result<u64, OutputError> {
    let mut total = 0u64;
    for path in walkdir(root)? {
        let meta = fs::symlink_metadata(path.as_std_path())?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}
