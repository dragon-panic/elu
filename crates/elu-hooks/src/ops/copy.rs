use camino::Utf8Path;

use crate::error::HookError;
use crate::path::{glob_in_staging, resolve_in_staging, verify_under_staging};

pub fn run(staging: &Utf8Path, from: &str, to: &str) -> Result<(), HookError> {
    let matches = glob_in_staging(staging, from)?;
    let dest_base = resolve_in_staging(staging, to)?;
    verify_under_staging(staging, &dest_base)?;
    for src in matches {
        let dest = if dest_base.as_str().ends_with('/') || dest_base.is_dir() {
            let name = src.file_name().unwrap_or("unknown");
            dest_base.join(name)
        } else {
            dest_base.clone()
        };
        verify_under_staging(staging, &dest)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest)?;
    }
    Ok(())
}
