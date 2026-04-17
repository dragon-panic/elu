use camino::{Utf8Path, Utf8PathBuf};
use elu_manifest::IndexFormat;

use crate::error::HookError;
use crate::ops::write::atomic_write;
use crate::path::{resolve_in_staging, verify_under_staging};

pub fn run(
    staging: &Utf8Path,
    root: &str,
    output: &str,
    format: &IndexFormat,
) -> Result<(), HookError> {
    let root_path = resolve_in_staging(staging, root)?;
    let dest = resolve_in_staging(staging, output)?;
    verify_under_staging(staging, &root_path)?;
    verify_under_staging(staging, &dest)?;

    let mut entries: Vec<(String, String)> = Vec::new();
    for entry in walkdir::WalkDir::new(root_path.as_std_path()).sort_by_file_name() {
        let entry = entry.map_err(|e| HookError::Io(e.into()))?;
        if entry.file_type().is_file() {
            let full = entry.path();
            let rel = full.strip_prefix(root_path.as_std_path())
                .map_err(|_| HookError::PathEscape(full.display().to_string()))?;
            let rel_utf8 = Utf8PathBuf::try_from(rel.to_path_buf())
                .map_err(|e| HookError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

            let mut hasher = elu_store::hasher::Hasher::new();
            let data = std::fs::read(full)?;
            hasher.update(&data);
            let hash = hasher.finalize();
            entries.push((rel_utf8.to_string(), hash.to_string()));
        }
    }

    let bytes = match format {
        IndexFormat::Sha256List => {
            let mut out = String::new();
            for (path, hash) in &entries {
                out.push_str(hash);
                out.push_str("  ");
                out.push_str(path);
                out.push('\n');
            }
            out.into_bytes()
        }
        IndexFormat::Json => {
            let map: Vec<serde_json::Value> = entries
                .iter()
                .map(|(p, h)| serde_json::json!({"path": p, "hash": h}))
                .collect();
            let mut bytes = serde_json::to_vec_pretty(&map)
                .map_err(|e| HookError::Io(std::io::Error::other(e)))?;
            bytes.push(b'\n');
            bytes
        }
        IndexFormat::Toml => {
            let mut out = String::new();
            for (path, hash) in &entries {
                out.push_str(&format!("[[file]]\npath = {path:?}\nhash = {hash:?}\n\n"));
            }
            out.into_bytes()
        }
    };
    atomic_write(&dest, &bytes)?;
    Ok(())
}
