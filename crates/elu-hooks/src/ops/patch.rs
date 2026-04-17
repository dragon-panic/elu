use camino::Utf8Path;
use elu_manifest::PatchSource;

use crate::error::HookError;
use crate::ops::write::atomic_write;
use crate::path::{resolve_in_staging, verify_under_staging};

pub fn run(staging: &Utf8Path, file: &str, source: &PatchSource) -> Result<(), HookError> {
    let target = resolve_in_staging(staging, file)?;
    verify_under_staging(staging, &target)?;

    let diff_text = match source {
        PatchSource::Inline { diff } => diff.clone(),
        PatchSource::File { from } => {
            let src = resolve_in_staging(staging, from)?;
            verify_under_staging(staging, &src)?;
            std::fs::read_to_string(&src)?
        }
    };

    let original = std::fs::read_to_string(&target)?;
    let patch = diffy::Patch::from_str(&diff_text)
        .map_err(|e| HookError::Diffy(e.to_string()))?;
    let patched = diffy::apply(&original, &patch)
        .map_err(|_| HookError::PatchFailed(target.clone()))?;
    atomic_write(&target, patched.as_bytes())?;
    Ok(())
}
