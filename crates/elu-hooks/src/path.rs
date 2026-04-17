use camino::{Utf8Path, Utf8PathBuf};
use globset::GlobBuilder;

use crate::error::HookError;

/// Resolve `rel` against `staging`, rejecting any path that escapes staging.
pub fn resolve_in_staging(staging: &Utf8Path, rel: &str) -> Result<Utf8PathBuf, HookError> {
    if rel.contains('\0') {
        return Err(HookError::PathEscape(rel.to_string()));
    }
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err(HookError::PathEscape(rel.to_string()));
    }
    // Reject Windows drive prefixes like C:
    if rel.len() >= 2 && rel.as_bytes()[0].is_ascii_alphabetic() && rel.as_bytes()[1] == b':' {
        return Err(HookError::PathEscape(rel.to_string()));
    }

    // Normalize .. components manually to detect escape
    let mut parts: Vec<&str> = Vec::new();
    for component in rel.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                if parts.is_empty() {
                    return Err(HookError::PathEscape(rel.to_string()));
                }
                parts.pop();
            }
            c => parts.push(c),
        }
    }

    let mut result = staging.to_path_buf();
    for part in &parts {
        result.push(part);
    }
    Ok(result)
}

/// Verify that `path` (which may have been followed through symlinks)
/// still resolves under staging. For use before writes.
pub fn verify_under_staging(staging: &Utf8Path, path: &Utf8Path) -> Result<(), HookError> {
    let canonical_staging = staging
        .as_std_path()
        .canonicalize()
        .map_err(|_| HookError::PathEscape(staging.to_string()))?;
    // If the path doesn't exist yet, check its parent
    let to_check = if path.as_std_path().exists() {
        path.as_std_path().canonicalize()
    } else if let Some(parent) = path.parent() {
        if parent.as_std_path().exists() {
            parent
                .as_std_path()
                .canonicalize()
                .map(|p| p.join(path.file_name().unwrap_or("")))
        } else {
            // Parent doesn't exist either — it'll be created under staging
            return Ok(());
        }
    } else {
        return Ok(());
    };
    match to_check {
        Ok(canonical) => {
            if !canonical.starts_with(&canonical_staging) {
                return Err(HookError::PathEscape(path.to_string()));
            }
            Ok(())
        }
        Err(_) => Ok(()), // path doesn't exist yet, that's fine
    }
}

/// Expand a glob pattern rooted at staging, returning only safe paths.
pub fn glob_in_staging(staging: &Utf8Path, pattern: &str) -> Result<Vec<Utf8PathBuf>, HookError> {
    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| HookError::Glob(e.to_string()))?;
    let matcher = glob.compile_matcher();

    let mut results = Vec::new();
    for entry in walkdir::WalkDir::new(staging.as_std_path()).sort_by_file_name() {
        let entry = entry.map_err(|e| HookError::Io(e.into()))?;
        let full_path = entry.path();
        if let Ok(rel) = full_path.strip_prefix(staging.as_std_path()) {
            let rel_str = rel.to_string_lossy();
            if matcher.is_match(rel_str.as_ref())
                && let Ok(utf8) = Utf8PathBuf::try_from(full_path.to_path_buf())
            {
                results.push(utf8);
            }
        }
    }
    Ok(results)
}
