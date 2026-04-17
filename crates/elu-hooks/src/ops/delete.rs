use camino::Utf8Path;

use crate::error::HookError;
use crate::path::glob_in_staging;

pub fn run(staging: &Utf8Path, paths: &[String]) -> Result<(), HookError> {
    for pat in paths {
        for path in glob_in_staging(staging, pat)? {
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}
