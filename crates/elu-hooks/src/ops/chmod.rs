use camino::Utf8Path;

use crate::error::HookError;
use crate::mode::ModeSpec;
use crate::path::glob_in_staging;

pub fn run(staging: &Utf8Path, paths: &[String], mode: &str) -> Result<(), HookError> {
    let parsed = ModeSpec::parse(mode)?;
    for pat in paths {
        let matches = glob_in_staging(staging, pat)?;
        for path in matches {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = std::fs::metadata(&path)?;
                let cur = meta.permissions().mode();
                let new = parsed.apply(cur);
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(new))?;
            }
            #[cfg(not(unix))]
            {
                let _ = (&path, &parsed);
            }
        }
    }
    Ok(())
}
