use elu_import::apt::AptImporter;
use elu_import::cache::Cache;
use elu_import::fetch::HttpFetcher;
use elu_import::npm::NpmImporter;
use elu_import::pip::PipImporter;
use elu_import::{ImportOptions, Importer};

use crate::cli::{ImportArgs, ImportKind};
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::output::emit_event;

pub fn run(ctx: &GlobalCtx, args: ImportArgs) -> Result<(), CliError> {
    let store = ctx.open_store()?;
    let cache_root = ctx.store_path().join("cache");
    let cache = Cache::new(cache_root.as_std_path()).map_err(CliError::from)?;
    let fetcher = HttpFetcher::new();

    let opts = ImportOptions {
        version: args.version.clone(),
        closure: args.closure,
        dist: args.dist.clone(),
        target: args.target.clone(),
    };

    if args.names.len() > 1 && args.version.is_some() {
        return Err(CliError::Usage(
            "--version cannot be combined with multiple package names".into(),
        ));
    }

    for name in &args.names {
        emit_event(
            ctx,
            &serde_json::json!({"event": "fetch", "kind": format!("{:?}", args.kind).to_lowercase(), "name": name}),
        );
        let hash = match args.kind {
            ImportKind::Apt => AptImporter.import(name, &opts, &store, &cache, &fetcher)?,
            ImportKind::Npm => NpmImporter.import(name, &opts, &store, &cache, &fetcher)?,
            ImportKind::Pip => PipImporter.import(name, &opts, &store, &cache, &fetcher)?,
        };
        if ctx.json {
            emit_event(
                ctx,
                &serde_json::json!({
                    "event": "imported",
                    "name": name,
                    "manifest_hash": hash.to_string(),
                }),
            );
        } else {
            println!("imported {name} -> {hash}");
        }
    }
    if ctx.json {
        let payload = serde_json::json!({"event": "done", "ok": true});
        println!("{payload}");
    }
    Ok(())
}
