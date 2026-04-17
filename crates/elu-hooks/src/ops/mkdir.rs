use camino::Utf8Path;

use crate::error::HookError;
use crate::path::{resolve_in_staging, verify_under_staging};

pub fn run(staging: &Utf8Path, path: &str, mode: Option<&str>, parents: bool) -> Result<(), HookError> {
    let dest = resolve_in_staging(staging, path)?;
    verify_under_staging(staging, &dest)?;
    if parents {
        std::fs::create_dir_all(&dest)?;
    } else {
        match std::fs::create_dir(&dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    #[cfg(unix)]
    if let Some(m) = mode {
        use std::os::unix::fs::PermissionsExt;
        let parsed = crate::mode::ModeSpec::parse(m)?;
        let cur = std::fs::metadata(&dest)?.permissions().mode();
        let new = parsed.apply(cur);
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(new))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}
