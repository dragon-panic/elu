use camino::Utf8Path;

use crate::error::HookError;
use crate::path::{resolve_in_staging, verify_under_staging};

pub fn run(staging: &Utf8Path, from: &str, to: &str, replace: bool) -> Result<(), HookError> {
    let link = resolve_in_staging(staging, from)?;
    verify_under_staging(staging, &link)?;
    // `to` is NOT resolved — symlink targets are relative-to-link or absolute-at-runtime
    if link.exists() || link.as_std_path().read_link().is_ok() {
        if !replace {
            return Err(HookError::SymlinkExists(link));
        }
        // Remove existing file or symlink
        if link.as_std_path().read_link().is_ok() {
            std::fs::remove_file(&link)?;
        } else if link.is_dir() {
            std::fs::remove_dir_all(&link)?;
        } else {
            std::fs::remove_file(&link)?;
        }
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(to, &link)?;
    #[cfg(not(unix))]
    {
        let _ = to;
        return Err(HookError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlink not supported on this platform",
        )));
    }
    Ok(())
}
