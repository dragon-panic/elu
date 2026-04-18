use std::fs;
use std::io::Cursor;

use camino::Utf8Path;
use elu_manifest::validate::{validate_source, validate_stored};
use elu_manifest::{from_toml_str, to_canonical_json, HookOp, Layer, Manifest};
use elu_store::hash::ManifestHash;
use elu_store::store::Store;

use crate::report::{from_manifest_err, Diagnostic, ErrorCode, Report};
use crate::sensitive::scan_paths;
use crate::tar_det::{build_deterministic_tar, TarEntry};
use crate::walk::{walk_layer, WalkOpts};

#[derive(Debug, Default, Clone)]
pub struct BuildOpts {
    pub check_only: bool,
    pub strict: bool,
}

#[derive(Debug)]
pub struct BuildArtifact {
    pub manifest_hash: ManifestHash,
    pub manifest: Manifest,
}

pub fn build(
    project_root: &Utf8Path,
    store: &dyn Store,
    opts: &BuildOpts,
) -> Result<(Report, Option<BuildArtifact>), Diagnostic> {
    let manifest_path = project_root.join("elu.toml");
    let src = fs::read_to_string(manifest_path.as_std_path()).map_err(|e| {
        Diagnostic::new(
            "elu.toml",
            ErrorCode::FileNotReadable,
            format!("cannot read elu.toml: {e}"),
        )
    })?;

    let mut report = Report::success();

    let source: Manifest = match from_toml_str(&src) {
        Ok(m) => m,
        Err(e) => {
            let mut d = from_manifest_err(&e);
            d.file = Some("elu.toml".into());
            report.push_error(d);
            return Ok((report, None));
        }
    };

    if let Err(e) = validate_source(&source) {
        let mut d = from_manifest_err(&e);
        d.file = Some("elu.toml".into());
        report.push_error(d);
        return Ok((report, None));
    }

    let mut stored_layers: Vec<Layer> = Vec::with_capacity(source.layers.len());
    let mut all_produced_paths: Vec<String> = Vec::new();

    for (idx, layer) in source.layers.iter().enumerate() {
        let resolved = match walk_layer(project_root, layer, &WalkOpts::default()) {
            Ok(r) => r,
            Err(mut d) => {
                d.field = format!("layer[{idx}]");
                report.push_error(d);
                return Ok((report, None));
            }
        };

        if resolved.is_empty() {
            report.push_error(
                Diagnostic::new(
                    format!("layer[{idx}].include"),
                    ErrorCode::LayerIncludeNoMatches,
                    format!(
                        "layer {idx} include patterns matched zero files: {:?}",
                        layer.include
                    ),
                )
                .with_hint("run your build step first, or correct the include patterns")
                .with_file("elu.toml"),
            );
            continue;
        }

        // Sensitive-pattern scan
        let refs: Vec<&str> = resolved.iter().map(|e| e.layer_path.as_str()).collect();
        for hit in scan_paths(&refs) {
            report.push_warning(
                Diagnostic::new(
                    format!("layer[{idx}]"),
                    ErrorCode::SensitivePattern,
                    format!("sensitive file: {} (matched {})", hit.path, hit.pattern),
                )
                .with_hint("add to exclude or remove from include"),
            );
        }

        all_produced_paths.extend(resolved.iter().map(|e| e.layer_path.clone()));

        if opts.check_only {
            // Skip packing
            continue;
        }

        let tar_entries: Vec<TarEntry> = resolved
            .into_iter()
            .map(|r| TarEntry::file(r.fs_path, r.layer_path, r.mode))
            .collect();
        let tar_bytes = match build_deterministic_tar(&tar_entries) {
            Ok(b) => b,
            Err(d) => {
                report.push_error(d);
                return Ok((report, None));
            }
        };

        let size = tar_bytes.len() as u64;
        let mut cursor = Cursor::new(tar_bytes);
        let put = store.put_blob(&mut cursor).map_err(|e| {
            Diagnostic::new("", ErrorCode::StoreError, format!("put_blob: {e}"))
        })?;

        stored_layers.push(Layer {
            diff_id: Some(put.diff_id),
            size: Some(size),
            name: layer.name.clone(),
            include: vec![],
            exclude: vec![],
            strip: None,
            place: None,
            mode: None,
            follow_symlinks: false,
        });
    }

    // Hook op path pre-check (single-package)
    validate_hook_op_paths(&source.hook.ops, &all_produced_paths, &mut report);

    if !report.ok {
        return Ok((report, None));
    }

    if opts.strict {
        report.promote_warnings();
        if !report.ok {
            return Ok((report, None));
        }
    }

    if opts.check_only {
        return Ok((report, None));
    }

    let stored = Manifest {
        schema: source.schema,
        package: source.package.clone(),
        layers: stored_layers,
        dependencies: source.dependencies.clone(),
        hook: source.hook.clone(),
        metadata: source.metadata.clone(),
    };

    if let Err(e) = validate_stored(&stored) {
        let mut d = from_manifest_err(&e);
        d.file = Some("elu.toml".into());
        report.push_error(d);
        return Ok((report, None));
    }

    let bytes = to_canonical_json(&stored);
    let manifest_hash = store.put_manifest(&bytes).map_err(|e| {
        Diagnostic::new("", ErrorCode::StoreError, format!("put_manifest: {e}"))
    })?;
    store
        .put_ref(
            &stored.package.namespace,
            &stored.package.name,
            &stored.package.version.to_string(),
            &manifest_hash,
        )
        .map_err(|e| Diagnostic::new("", ErrorCode::StoreError, format!("put_ref: {e}")))?;

    Ok((
        report,
        Some(BuildArtifact {
            manifest_hash,
            manifest: stored,
        }),
    ))
}

fn validate_hook_op_paths(ops: &[HookOp], produced: &[String], report: &mut Report) {
    for (idx, op) in ops.iter().enumerate() {
        match op {
            HookOp::Chmod { paths, .. } | HookOp::Delete { paths } => {
                for p in paths {
                    if !produced.iter().any(|q| q == p)
                        && !produced.iter().any(|q| glob_match(p, q))
                    {
                        report.push_warning(
                            Diagnostic::new(
                                format!("hook.op[{idx}]"),
                                ErrorCode::HookOpPathNotProduced,
                                format!("path {p} not produced by this package; cross-package paths are not validated in v1"),
                            )
                            .with_hint(
                                "if the path comes from a dependency layer, this is informational",
                            ),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn glob_match(pattern: &str, path: &str) -> bool {
    globset::Glob::new(pattern)
        .ok()
        .map(|g| g.compile_matcher().is_match(path))
        .unwrap_or(false)
}
