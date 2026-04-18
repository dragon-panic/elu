use std::io::Cursor;

use camino::Utf8Path;
use elu_manifest::Manifest;
use elu_store::store::Store;
use sha2::{Digest, Sha256};

use crate::report::{Diagnostic, ErrorCode};
use crate::tar_det::{build_deterministic_tar, TarEntry};
use crate::walk::{walk_layer, WalkOpts};

#[derive(Debug, Default)]
pub struct LayerFingerprints {
    fps: Vec<Option<String>>,
}

impl LayerFingerprints {
    pub fn len(&self) -> usize {
        self.fps.iter().filter(|f| f.is_some()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, idx: usize) -> Option<&String> {
        self.fps.get(idx).and_then(|o| o.as_ref())
    }

    fn set(&mut self, idx: usize, fp: String) {
        while self.fps.len() <= idx {
            self.fps.push(None);
        }
        self.fps[idx] = Some(fp);
    }
}

/// Walk each layer and repack only the layers whose fingerprint changed.
/// Returns the set of layer indices that were repacked this pass.
pub fn incremental_build(
    project_root: &Utf8Path,
    manifest: &Manifest,
    store: &dyn Store,
    fps: &mut LayerFingerprints,
) -> Result<Vec<usize>, Diagnostic> {
    let mut changed = Vec::new();
    for (idx, layer) in manifest.layers.iter().enumerate() {
        let resolved = walk_layer(project_root, layer, &WalkOpts::default())?;
        let new_fp = fingerprint(&resolved)?;
        let prev = fps.get(idx).cloned();
        if prev.as_deref() == Some(new_fp.as_str()) {
            continue;
        }

        let tar_entries: Vec<TarEntry> = resolved
            .into_iter()
            .map(|r| TarEntry::file(r.fs_path, r.layer_path, r.mode))
            .collect();
        let tar_bytes = build_deterministic_tar(&tar_entries)?;
        let mut cursor = Cursor::new(tar_bytes);
        store.put_blob(&mut cursor).map_err(|e| {
            Diagnostic::new("", ErrorCode::StoreError, format!("put_blob: {e}"))
        })?;

        fps.set(idx, new_fp);
        changed.push(idx);
    }
    Ok(changed)
}

fn fingerprint(resolved: &[crate::walk::ResolvedEntry]) -> Result<String, Diagnostic> {
    let mut tuples: Vec<(String, u64, i64)> = Vec::with_capacity(resolved.len());
    for r in resolved {
        let meta = std::fs::metadata(r.fs_path.as_std_path()).map_err(|e| {
            Diagnostic::new(
                "",
                ErrorCode::FileNotReadable,
                format!("stat {}: {e}", r.fs_path),
            )
        })?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        tuples.push((r.layer_path.clone(), meta.len(), mtime));
    }
    tuples.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (p, sz, mt) in &tuples {
        hasher.update(p.as_bytes());
        hasher.update(sz.to_le_bytes());
        hasher.update(mt.to_le_bytes());
        hasher.update(b"\n");
    }
    let out: [u8; 32] = hasher.finalize().into();
    let mut s = String::with_capacity(64);
    for b in out {
        use std::fmt::Write;
        write!(&mut s, "{b:02x}").ok();
    }
    Ok(s)
}

