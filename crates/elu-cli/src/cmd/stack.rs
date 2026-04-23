use camino::Utf8Path;
use elu_hooks::HookMode as LayersHookMode;
use elu_layers::{stage, stack as layers_stack};
use elu_manifest::types::{PackageRef, VersionSpec};
use elu_outputs::{
    Compression, DirOpts, FormatName, Options, Outcome, Qcow2Opts, TarOpts, infer_compression,
    infer_format, materialize, qcow2,
};
use elu_resolver::{OfflineSource, Resolution, RootRef, resolve};
use elu_store::store::{RefFilter, Store};
use semver::VersionReq;

use crate::cli::{
    CompressionKind, HookMode as CliHookMode, OutputFormat, StackArgs,
};
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::output::emit_event;
use crate::refs_parse::{Ref, parse_ref};

pub fn run(ctx: &GlobalCtx, args: StackArgs) -> Result<(), CliError> {
    if args.refs.len() != 1 {
        return Err(CliError::Usage(
            "stack accepts exactly one ref in v1 (multi-ref stacking will arrive with `install`)"
                .into(),
        ));
    }

    let format = resolve_format(&args)?;
    // qcow2 needs a --base; refuse early with a clear message.
    if format == FormatName::Qcow2 && args.base.is_none() {
        return Err(CliError::Usage(
            "qcow2 output requires --base <pkg-ref>".into(),
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
        _ => LayersHookMode::Safe,
    };

    let (layers_count, stats, outcome) =
        run_pipeline(&store, &resolution, &args, format, hook_mode)?;

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "done",
                "out": args.out.to_string(),
                "format": format_to_str(format),
                "layers": layers_count,
                "entries_applied": stats.apply.entries_applied,
                "whiteouts": stats.apply.whiteouts,
                "hook_ops_run": stats.hook.ops_run,
                "bytes": outcome.bytes,
            }),
        );
    } else {
        println!(
            "stacked {} layers ({} entries, {} whiteouts) into {} [{}, {} bytes]",
            layers_count,
            stats.apply.entries_applied,
            stats.apply.whiteouts,
            args.out,
            format_to_str(format),
            outcome.bytes
        );
    }
    Ok(())
}

fn run_pipeline(
    store: &dyn Store,
    resolution: &Resolution,
    args: &StackArgs,
    format: FormatName,
    hook_mode: LayersHookMode,
) -> Result<(u64, elu_layers::StackStats, Outcome), CliError> {
    // Dir with no post-walk options: keep the existing stack() fast-path
    // (which does stage + rename inline) so `elu stack foo -o ./foo` is
    // unchanged for simple users.
    if matches!(format, FormatName::Dir)
        && args.owner.is_none()
        && args.mode.is_none()
    {
        let stats = layers_stack(store, resolution, &args.out, hook_mode, args.force)
            .map_err(|e| CliError::Generic(format!("stack: {e}")))?;
        let bytes = dir_bytes(&args.out).unwrap_or(0);
        return Ok((stats.layers, stats, Outcome { bytes }));
    }

    let parent = target_parent(&args.out);
    std::fs::create_dir_all(parent.as_std_path())
        .map_err(|e| CliError::Generic(format!("create parent: {e}")))?;

    let (staging, stats) = stage(store, resolution, parent, hook_mode)
        .map_err(|e| CliError::Generic(format!("stage: {e}")))?;
    let staging_path = staging.into_path();

    let outcome = match format {
        FormatName::Qcow2 => {
            run_qcow2(store, args, &staging_path, hook_mode).inspect_err(|_| {
                let _ = std::fs::remove_dir_all(staging_path.as_std_path());
            })?
        }
        _ => {
            let options = build_options(format, args)?;
            match materialize(format, &staging_path, &args.out, &options) {
                Ok(o) => o,
                Err(e) => {
                    let _ = std::fs::remove_dir_all(staging_path.as_std_path());
                    return Err(CliError::Generic(format!("output: {e}")));
                }
            }
        }
    };

    Ok((stats.layers, stats, outcome))
}

fn run_qcow2(
    store: &dyn Store,
    args: &StackArgs,
    user_staging: &Utf8Path,
    hook_mode: LayersHookMode,
) -> Result<Outcome, CliError> {
    let base_ref = args
        .base
        .as_ref()
        .ok_or_else(|| CliError::Usage("qcow2 output requires --base".into()))?;
    let base_root = build_root_ref(base_ref)?;
    let source = build_offline_source(store)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio runtime: {e}")))?;
    let base_resolution: Resolution = runtime
        .block_on(resolve(&[base_root], &source, None, Some(store)))
        .map_err(|e| CliError::Resolution(format!("base: {e}")))?;

    if !base_resolution.fetch_plan.items.is_empty() {
        return Err(CliError::Resolution(format!(
            "base: {} blob(s) missing from local store",
            base_resolution.fetch_plan.items.len()
        )));
    }

    let base_manifest = &base_resolution
        .manifests
        .first()
        .ok_or_else(|| CliError::Generic("base resolution produced no manifests".into()))?
        .manifest;
    let base_meta = qcow2::parse_os_base(base_manifest)
        .map_err(|e| CliError::Generic(format!("base: {e}")))?;

    let parent = target_parent(&args.out);
    let (base_staging, _base_stats) = stage(store, &base_resolution, parent, hook_mode)
        .map_err(|e| CliError::Generic(format!("base stage: {e}")))?;
    let base_staging_path = base_staging.into_path();

    let opts = Qcow2Opts {
        force: args.force,
        size: args.size,
        format_version: args.format_version.unwrap_or(3),
        no_finalize: args.no_finalize,
    };
    let result = qcow2::materialize(user_staging, &base_staging_path, &base_meta, &args.out, &opts);
    let _ = std::fs::remove_dir_all(base_staging_path.as_std_path());
    result.map_err(|e| CliError::Generic(format!("qcow2: {e}")))
}

fn resolve_format(args: &StackArgs) -> Result<FormatName, CliError> {
    if let Some(explicit) = args.format {
        return Ok(match explicit {
            OutputFormat::Dir => FormatName::Dir,
            OutputFormat::Tar => FormatName::Tar,
            OutputFormat::Qcow2 => FormatName::Qcow2,
        });
    }
    infer_format(&args.out).ok_or_else(|| {
        CliError::Usage(format!(
            "cannot infer output format from '{}'; pass --format",
            args.out
        ))
    })
}

fn format_to_str(f: FormatName) -> &'static str {
    match f {
        FormatName::Dir => "dir",
        FormatName::Tar => "tar",
        FormatName::Qcow2 => "qcow2",
    }
}

fn build_options(format: FormatName, args: &StackArgs) -> Result<Options, CliError> {
    match format {
        FormatName::Dir => {
            let opts = DirOpts {
                force: args.force,
                owner: parse_owner(args.owner.as_deref())?,
                mode_mask: parse_mode(args.mode.as_deref())?,
            };
            Ok(Options::Dir(opts))
        }
        FormatName::Tar => {
            let compress = match args.compress {
                Some(CompressionKind::None) => Compression::None,
                Some(CompressionKind::Gzip) => Compression::Gzip,
                Some(CompressionKind::Zstd) => Compression::Zstd,
                Some(CompressionKind::Xz) => Compression::Xz,
                None => infer_compression(&args.out),
            };
            let opts = TarOpts {
                force: args.force,
                compress,
                level: args.level,
                deterministic: !args.no_deterministic,
            };
            Ok(Options::Tar(opts))
        }
        FormatName::Qcow2 => unreachable!("qcow2 handled by run_qcow2"),
    }
}

fn parse_owner(s: Option<&str>) -> Result<Option<(u32, u32)>, CliError> {
    let Some(s) = s else { return Ok(None) };
    let (u, g) = s
        .split_once(':')
        .ok_or_else(|| CliError::Usage(format!("--owner expects UID:GID, got '{s}'")))?;
    let uid: u32 = u
        .parse()
        .map_err(|_| CliError::Usage(format!("--owner uid: '{u}'")))?;
    let gid: u32 = g
        .parse()
        .map_err(|_| CliError::Usage(format!("--owner gid: '{g}'")))?;
    Ok(Some((uid, gid)))
}

fn parse_mode(s: Option<&str>) -> Result<Option<u32>, CliError> {
    let Some(s) = s else { return Ok(None) };
    let s = s.strip_prefix("0o").unwrap_or(s);
    u32::from_str_radix(s, 8)
        .map(Some)
        .map_err(|_| CliError::Usage(format!("--mode expects octal, got '{s}'")))
}

fn target_parent(target: &Utf8Path) -> &Utf8Path {
    target
        .parent()
        .filter(|p| !p.as_str().is_empty())
        .unwrap_or(Utf8Path::new("."))
}

fn dir_bytes(root: &Utf8Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir.as_std_path())? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let path = camino::Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|_| std::io::Error::other("non-utf8 path"))?;
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

fn build_root_ref(s: &str) -> Result<RootRef, CliError> {
    match parse_ref(s)? {
        Ref::Hash(hash) => {
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
