use elu_store::store::Store;

use crate::cli::GcArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::manifest_reader::ManifestParser;

pub fn run(ctx: &GlobalCtx, args: GcArgs) -> Result<(), CliError> {
    let store = ctx.open_store()?;
    if args.dry_run {
        let plan = store.plan_gc(&ManifestParser)?;
        if ctx.json {
            let payload = serde_json::json!({
                "event": "done",
                "ok": true,
                "dry_run": true,
                "objects_to_remove": plan.objects_to_remove.len() as u64,
                "diffs_to_remove": plan.diffs_to_remove.len() as u64,
                "tmp_to_remove": plan.tmp_to_remove.len() as u64,
                "bytes_to_free": plan.bytes_to_free,
            });
            println!("{payload}");
        } else {
            println!(
                "would remove {} objects, {} diffs, {} tmp; would free {} bytes",
                plan.objects_to_remove.len(),
                plan.diffs_to_remove.len(),
                plan.tmp_to_remove.len(),
                plan.bytes_to_free,
            );
        }
        return Ok(());
    }
    let stats = store.gc(&ManifestParser)?;
    if ctx.json {
        let payload = serde_json::json!({
            "event": "done",
            "ok": true,
            "objects_removed": stats.objects_removed,
            "diffs_removed": stats.diffs_removed,
            "tmp_removed": stats.tmp_removed,
            "bytes_freed": stats.bytes_freed,
        });
        println!("{payload}");
    } else {
        println!(
            "removed {} objects, {} diffs, {} tmp; freed {} bytes",
            stats.objects_removed, stats.diffs_removed, stats.tmp_removed, stats.bytes_freed
        );
    }
    Ok(())
}
