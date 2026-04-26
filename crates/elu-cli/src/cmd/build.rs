use std::sync::mpsc;
use std::time::Duration;

use camino::Utf8PathBuf;
use elu_author::build::{build, BuildOpts};
use elu_store::store::Store;
use notify::{RecursiveMode, Watcher};

use crate::cli::BuildArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::output::emit_event;

pub fn run(ctx: &GlobalCtx, args: BuildArgs) -> Result<(), CliError> {
    let project_root: Utf8PathBuf = match &args.manifest {
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
        force_ref: args.watch,
    };

    if args.watch {
        return run_watch(ctx, &project_root, &store, &opts);
    }

    let ok = build_once(ctx, &project_root, &store, &opts)?;
    if ok {
        Ok(())
    } else {
        Err(CliError::Usage("build failed".into()))
    }
}

fn run_watch(
    ctx: &GlobalCtx,
    project_root: &Utf8PathBuf,
    store: &dyn Store,
    opts: &BuildOpts,
) -> Result<(), CliError> {
    let _ = build_once(ctx, project_root, store, opts)?;

    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })
    .map_err(|e| CliError::Generic(format!("watch init: {e}")))?;
    watcher
        .watch(project_root.as_std_path(), RecursiveMode::Recursive)
        .map_err(|e| CliError::Generic(format!("watch start: {e}")))?;

    loop {
        match rx.recv() {
            Ok(()) => {
                // Debounce: drain pending events for 200ms before rebuilding.
                while rx.recv_timeout(Duration::from_millis(200)).is_ok() {}
                let _ = build_once(ctx, project_root, store, opts)?;
            }
            Err(_) => return Ok(()),
        }
    }
}

fn build_once(
    ctx: &GlobalCtx,
    project_root: &Utf8PathBuf,
    store: &dyn Store,
    opts: &BuildOpts,
) -> Result<bool, CliError> {
    let (report, artifact) = build(project_root, store, opts)?;

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
    Ok(ok)
}
