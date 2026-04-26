use std::os::unix::fs::PermissionsExt;

use camino::{Utf8Path, Utf8PathBuf};
use elu_manifest::Layer;
use globset::{Glob, GlobSetBuilder};

use crate::report::{Diagnostic, ErrorCode};

/// One resolved file ready for tar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedEntry {
    pub fs_path: Utf8PathBuf,
    pub layer_path: String,
    pub mode: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct WalkOpts {
    pub follow_symlinks: bool,
}

pub fn walk_layer(
    root: &Utf8Path,
    layer: &Layer,
    opts: &WalkOpts,
) -> Result<Vec<ResolvedEntry>, Diagnostic> {
    for pat in layer.include.iter().chain(layer.exclude.iter()) {
        if pat.starts_with('/') {
            return Err(Diagnostic::new(
                "layer.include",
                ErrorCode::LayerAbsolutePath,
                format!("absolute path not allowed: {pat}"),
            )
            .with_hint("use a path relative to the project root"));
        }
        if pat.split('/').any(|seg| seg == "..") {
            return Err(Diagnostic::new(
                "layer.include",
                ErrorCode::LayerParentEscape,
                format!("`..` not allowed in patterns: {pat}"),
            ));
        }
    }
    for (field, value) in [("strip", layer.strip.as_deref()), ("place", layer.place.as_deref())] {
        let Some(s) = value else { continue };
        if s.starts_with('/') {
            return Err(Diagnostic::new(
                format!("layer.{field}"),
                ErrorCode::LayerAbsolutePath,
                format!("absolute path not allowed: {s}"),
            ));
        }
        if s.split('/').any(|seg| seg == "..") {
            return Err(Diagnostic::new(
                format!("layer.{field}"),
                ErrorCode::LayerParentEscape,
                format!("`..` not allowed: {s}"),
            ));
        }
    }

    let include = build_globset(&layer.include)?;
    let exclude = build_globset(&layer.exclude)?;
    let follow = opts.follow_symlinks || layer.follow_symlinks;

    let mut entries: Vec<ResolvedEntry> = Vec::new();

    let walker = walkdir::WalkDir::new(root.as_std_path()).follow_links(follow);
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() && !entry.file_type().is_symlink() {
            continue;
        }
        if entry.file_type().is_symlink() && follow {
            // walkdir follows — treat as file via metadata
        }
        let abs = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|_| Diagnostic::new("", ErrorCode::FileNotReadable, "non-utf8 path"))?;
        let rel = match abs.strip_prefix(root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => continue,
        };
        let rel_str = rel.as_str().to_string();
        if rel_str.is_empty() {
            continue;
        }
        if !include.is_match(&rel_str) {
            continue;
        }
        if !layer.exclude.is_empty() && exclude.is_match(&rel_str) {
            continue;
        }

        let layer_path = apply_strip_place(&rel_str, layer.strip.as_deref(), layer.place.as_deref());
        let mode = entry
            .metadata()
            .ok()
            .map(|m| m.permissions().mode() & 0o777);

        entries.push(ResolvedEntry {
            fs_path: abs,
            layer_path,
            mode,
        });
    }

    entries.sort_by(|a, b| a.layer_path.cmp(&b.layer_path));
    Ok(entries)
}

fn build_globset(patterns: &[String]) -> Result<globset::GlobSet, Diagnostic> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let g = Glob::new(p)
            .map_err(|e| Diagnostic::new("", ErrorCode::GlobInvalid, e.to_string()))?;
        b.add(g);
    }
    b.build()
        .map_err(|e| Diagnostic::new("", ErrorCode::GlobInvalid, e.to_string()))
}

fn apply_strip_place(rel: &str, strip: Option<&str>, place: Option<&str>) -> String {
    let after_strip = match strip {
        Some(pfx) if rel.starts_with(pfx) => &rel[pfx.len()..],
        _ => rel,
    };
    match place {
        Some(pfx) => {
            let mut s = pfx.to_string();
            if !s.ends_with('/') && !after_strip.is_empty() {
                s.push('/');
            }
            s.push_str(after_strip);
            s
        }
        None => after_strip.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_then_place_single_file() {
        assert_eq!(
            apply_strip_place("target/release/hello", Some("target/release/"), Some("bin/")),
            "bin/hello"
        );
    }

    #[test]
    fn place_without_strip() {
        assert_eq!(
            apply_strip_place("README.md", None, Some("share/doc/foo/")),
            "share/doc/foo/README.md"
        );
    }

    #[test]
    fn neither_strip_nor_place_is_identity() {
        assert_eq!(apply_strip_place("src/lib.rs", None, None), "src/lib.rs");
    }
}
