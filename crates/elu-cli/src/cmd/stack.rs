use elu_hooks::HookMode as LayersHookMode;
use elu_layers::stack as layers_stack;
use elu_manifest::types::{PackageRef, VersionSpec};
use elu_resolver::{OfflineSource, Resolution, RootRef, resolve};
use elu_store::store::{RefFilter, Store};
use semver::VersionReq;

use crate::cli::{HookMode as CliHookMode, StackArgs};
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::output::emit_event;
use crate::refs_parse::{Ref, parse_ref};

pub fn run(ctx: &GlobalCtx, args: StackArgs) -> Result<(), CliError> {
    if args.format.is_some() || args.base.is_some() {
        return Err(CliError::Generic(
            "--format and --base require output formats other than dir; not in v1 stacker".into(),
        ));
    }
    if args.refs.len() != 1 {
        return Err(CliError::Usage(
            "stack accepts exactly one ref in v1 (multi-ref stacking will arrive with `install`)"
                .into(),
        ));
    }

    let store = ctx.open_store()?;
    let root = build_root_ref(&args.refs[0])?;
    let source = build_offline_source(&store)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio runtime: {e}")))?;
    let resolution: Resolution = runtime
        .block_on(resolve(&[root], &source, None, Some(&store)))
        .map_err(|e| CliError::Resolution(e.to_string()))?;

    if !resolution.fetch_plan.items.is_empty() {
        return Err(CliError::Resolution(format!(
            "{} blob(s) missing from local store; run `elu install` (WKIW.wX0h) to fetch",
            resolution.fetch_plan.items.len()
        )));
    }

    let hook_mode = match ctx.hooks {
        Some(CliHookMode::Off) => LayersHookMode::Off,
        // v1 stacker honors Off vs everything-else; Ask/Trust UX is policy work.
        _ => LayersHookMode::Safe,
    };

    let stats = layers_stack(&store, &resolution, &args.out, hook_mode, false)
        .map_err(|e| CliError::Generic(format!("stack: {e}")))?;

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "done",
                "out": args.out.to_string(),
                "layers": stats.layers,
                "entries_applied": stats.apply.entries_applied,
                "whiteouts": stats.apply.whiteouts,
                "hook_ops_run": stats.hook.ops_run,
            }),
        );
    } else {
        println!(
            "stacked {} layers ({} entries, {} whiteouts) into {}",
            stats.layers, stats.apply.entries_applied, stats.apply.whiteouts, args.out
        );
    }
    Ok(())
}

fn build_root_ref(s: &str) -> Result<RootRef, CliError> {
    match parse_ref(s)? {
        Ref::Hash(hash) => {
            // A hash-form ref is its own pin; package name doesn't matter for
            // resolve_one's hash branch but the resolver still uses it for
            // conflict tracking. Use a placeholder; this is fine for a single
            // root with no version-range deps.
            let package: PackageRef = "local/root".parse().map_err(CliError::Usage)?;
            Ok(RootRef {
                package,
                version: VersionSpec::Pinned(hash),
            })
        }
        Ref::Exact { namespace, name, version } => {
            let package: PackageRef = format!("{namespace}/{name}")
                .parse()
                .map_err(CliError::Usage)?;
            let req = VersionReq::parse(&format!("={version}"))
                .map_err(|e| CliError::Usage(format!("version req: {e}")))?;
            Ok(RootRef {
                package,
                version: VersionSpec::Range(req),
            })
        }
    }
}

fn build_offline_source(store: &dyn Store) -> Result<OfflineSource, CliError> {
    let mut source = OfflineSource::new();
    for entry in store.list_refs(RefFilter::default())? {
        let bytes = store
            .get_manifest(&entry.hash)?
            .ok_or_else(|| CliError::Store(format!("manifest blob missing: {}", entry.hash)))?;
        let manifest = parse_manifest(&bytes)?;
        source.insert(manifest, entry.hash);
    }
    Ok(source)
}

fn parse_manifest(bytes: &[u8]) -> Result<elu_manifest::Manifest, CliError> {
    if let Ok(m) = serde_json::from_slice::<elu_manifest::Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes)
        .map_err(|_| CliError::Store("manifest is not utf-8".into()))?;
    elu_manifest::from_toml_str(s).map_err(CliError::from)
}
