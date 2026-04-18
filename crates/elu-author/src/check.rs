use std::fs;

use camino::Utf8Path;
use elu_manifest::from_toml_str;
use elu_manifest::validate::validate_source;

use crate::report::{from_manifest_err, Diagnostic, ErrorCode, Report};
use crate::sensitive::scan_paths;
use crate::walk::{walk_layer, WalkOpts};

#[derive(Debug, Default, Clone)]
pub struct CheckOpts {
    pub strict: bool,
}

/// Validate an `elu.toml` without building layer blobs.
/// Emits structured diagnostics for every failure.
pub fn check(project_root: &Utf8Path, opts: &CheckOpts) -> Report {
    let mut report = Report::success();
    let manifest_path = project_root.join("elu.toml");
    let src = match fs::read_to_string(manifest_path.as_std_path()) {
        Ok(s) => s,
        Err(e) => {
            report.push_error(
                Diagnostic::new(
                    "elu.toml",
                    ErrorCode::FileNotReadable,
                    format!("cannot read elu.toml: {e}"),
                )
                .with_file("elu.toml"),
            );
            return report;
        }
    };

    let source = match from_toml_str(&src) {
        Ok(m) => m,
        Err(e) => {
            let mut d = from_manifest_err(&e);
            d.file = Some("elu.toml".into());
            report.push_error(d);
            return report;
        }
    };

    if let Err(e) = validate_source(&source) {
        let mut d = from_manifest_err(&e);
        d.file = Some("elu.toml".into());
        report.push_error(d);
        return report;
    }

    let mut produced_paths: Vec<String> = Vec::new();
    for (idx, layer) in source.layers.iter().enumerate() {
        let resolved = match walk_layer(project_root, layer, &WalkOpts::default()) {
            Ok(r) => r,
            Err(mut d) => {
                d.field = format!("layer[{idx}]");
                d.file = Some("elu.toml".into());
                report.push_error(d);
                continue;
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
                .with_hint("run your build step first, or correct the patterns")
                .with_file("elu.toml"),
            );
            continue;
        }
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
        produced_paths.extend(resolved.iter().map(|e| e.layer_path.clone()));
    }

    // Hook-op path precheck (informational — single-package scope only)
    hook_op_path_warnings(&source.hook.ops, &produced_paths, &mut report);

    if opts.strict {
        report.promote_warnings();
    }

    report
}

fn hook_op_path_warnings(
    ops: &[elu_manifest::HookOp],
    produced: &[String],
    report: &mut Report,
) {
    for (idx, op) in ops.iter().enumerate() {
        match op {
            elu_manifest::HookOp::Chmod { paths, .. }
            | elu_manifest::HookOp::Delete { paths } => {
                for p in paths {
                    if !produced.iter().any(|q| q == p)
                        && !produced.iter().any(|q| glob_match(p, q))
                    {
                        report.push_warning(
                            Diagnostic::new(
                                format!("hook.op[{idx}]"),
                                ErrorCode::HookOpPathNotProduced,
                                format!(
                                    "path {p} not produced by this package; cross-package paths are not validated in v1"
                                ),
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
