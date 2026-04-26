use elu_store::store::{FsckError, Store};

use crate::cli::FsckArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(ctx: &GlobalCtx, args: FsckArgs) -> Result<(), CliError> {
    let store = ctx.open_store()?;
    if args.repair {
        let report = store.fsck_repair()?;
        if ctx.json {
            let payload = serde_json::json!({
                "event": "done",
                "ok": true,
                "repair": true,
                "orphaned_diffs_removed": report.orphaned_diffs_removed,
                "broken_refs_removed": report.broken_refs_removed,
            });
            println!("{payload}");
        } else {
            println!(
                "repaired: {} orphaned diffs, {} broken refs",
                report.orphaned_diffs_removed, report.broken_refs_removed,
            );
        }
        return Ok(());
    }
    let errors = store.fsck()?;
    if ctx.json {
        let payload = serde_json::json!({
            "event": "done",
            "ok": errors.is_empty(),
            "errors": errors.iter().map(describe).collect::<Vec<_>>(),
        });
        println!("{payload}");
    } else {
        for e in &errors {
            eprintln!("fsck: {}", describe(e));
        }
        if errors.is_empty() {
            println!("ok");
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CliError::Store(format!("{} fsck errors", errors.len())))
    }
}

fn describe(e: &FsckError) -> String {
    match e {
        FsckError::HashMismatch { path, expected, actual } => {
            format!("hash-mismatch {path}: expected {expected} actual {actual}")
        }
        FsckError::OrphanedDiff { diff_id, blob_id } => {
            format!("orphaned-diff {diff_id} -> {blob_id}")
        }
        FsckError::BrokenRef { ref_path, target } => {
            format!("broken-ref {ref_path} -> {target}")
        }
    }
}
