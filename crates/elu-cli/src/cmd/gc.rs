use elu_store::store::Store;

use crate::cli::GcArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::manifest_reader::ManifestParser;

pub fn run(ctx: &GlobalCtx, args: GcArgs) -> Result<(), CliError> {
    if args.dry_run {
        return Err(CliError::Generic(
            "gc --dry-run not implemented in v1 (Store::gc has no dry-run mode)".into(),
        ));
    }
    let store = ctx.open_store()?;
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
