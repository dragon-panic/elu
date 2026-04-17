use std::collections::BTreeMap;

use camino::Utf8Path;

use crate::error::HookError;
use crate::interpolate::interpolate;
use crate::path::{resolve_in_staging, verify_under_staging};
use crate::PackageContext;

pub fn run(
    staging: &Utf8Path,
    pkg: &PackageContext,
    path: &str,
    content: &str,
    mode: Option<&str>,
    replace: bool,
) -> Result<(), HookError> {
    let dest = resolve_in_staging(staging, path)?;
    verify_under_staging(staging, &dest)?;
    if dest.exists() && !replace {
        return Err(HookError::FileExists(dest));
    }
    let interp = interpolate(content, pkg, &BTreeMap::new())?;
    atomic_write(&dest, interp.as_bytes())?;
    #[cfg(unix)]
    if let Some(m) = mode {
        use std::os::unix::fs::PermissionsExt;
        let parsed = crate::mode::ModeSpec::parse(m)?;
        let new = parsed.apply(0o644);
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(new))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

pub(crate) fn atomic_write(path: &Utf8Path, data: &[u8]) -> Result<(), HookError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let parent = path.parent().expect("path must have a parent");
    let tmp = tempfile::NamedTempFile::new_in(parent.as_std_path())?;
    std::fs::write(tmp.path(), data)?;
    tmp.persist(path.as_std_path()).map_err(|e| HookError::Io(e.error))?;
    Ok(())
}
