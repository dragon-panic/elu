use camino::Utf8PathBuf;
use elu_author::build::{build, BuildOpts};

use crate::cli::BuildArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::output::emit_event;

pub fn run(ctx: &GlobalCtx, args: BuildArgs) -> Result<(), CliError> {
    if args.watch {
        return Err(CliError::Generic("build --watch not implemented in v1".into()));
    }
    let project_root: Utf8PathBuf = match args.manifest {
        Some(p) => p
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| Utf8PathBuf::from(".")),
        None => Utf8PathBuf::from("."),
    };
    let store = ctx.open_store()?;
    let opts = BuildOpts {
        check_only: args.check,
        strict: args.strict,
    };
    let (report, artifact) = build(&project_root, &store, &opts)?;

    for d in &report.errors {
        if ctx.json {
            emit_event(
                ctx,
                &serde_json::json!({"event": "diagnostic", "severity": "error", "code": d.code, "field": d.field, "message": d.message}),
            );
        } else {
            eprintln!("error[{}]: {}: {}", d.code, d.field, d.message);
            if !d.hint.is_empty() {
                eprintln!("  hint: {}", d.hint);
            }
        }
    }
    for d in &report.warnings {
        if ctx.json {
            emit_event(
                ctx,
                &serde_json::json!({"event": "diagnostic", "severity": "warning", "code": d.code, "field": d.field, "message": d.message}),
            );
        } else {
            eprintln!("warning[{}]: {}: {}", d.code, d.field, d.message);
        }
    }

    let ok = report.ok;
    let manifest_hash = artifact.as_ref().map(|a| a.manifest_hash.to_string());
    if ctx.json {
        let mut payload = serde_json::json!({
            "event": "done",
            "ok": ok,
        });
        if let Some(h) = &manifest_hash {
            payload["manifest_hash"] = serde_json::Value::String(h.clone());
        }
        println!("{payload}");
    } else if let Some(h) = &manifest_hash {
        println!("built {h}");
    }

    if ok {
        Ok(())
    } else {
        Err(CliError::Usage("build failed".into()))
    }
}
