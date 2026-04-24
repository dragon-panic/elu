//! `elu install` — fetch a package from the configured registry into the
//! local store, then materialize it into an output directory.
//!
//! Slice 3 of the registry round-trip arc (cx SnIt). v1 is intentionally
//! narrow: a single explicit `<ns>/<name>@<version>` ref is fetched from the
//! registry and stacked into `--out` (default `./elu-out`). Range refs and
//! transitive registry resolution land in later slices; if the resolver
//! still wants more after install populates the store, install errors out
//! pointing at that gap.

use std::io::Cursor;

use camino::Utf8PathBuf;
use elu_hooks::HookMode as LayersHookMode;
use elu_layers::stack as layers_stack;
use elu_manifest::types::{PackageRef, VersionSpec};
use elu_registry::client::fallback::RegistryClient;
use elu_registry::client::verify::{verify_blob, verify_manifest};
use elu_registry::types::PackageRecord;
use elu_resolver::{OfflineSource, Resolution, RootRef, resolve};
use elu_store::store::{RefFilter, Store};
use semver::VersionReq;

use crate::cli::{HookMode as CliHookMode, InstallArgs};
use crate::error::CliError;
use crate::global::{DEFAULT_REGISTRY, GlobalCtx};
use crate::output::emit_event;
use crate::refs_parse::{Ref, parse_ref};

pub fn run(ctx: &GlobalCtx, args: InstallArgs) -> Result<(), CliError> {
    if ctx.offline {
        return Err(CliError::Network(
            "--offline forbids registry contact (install needs to fetch)".into(),
        ));
    }
    if args.refs.is_empty() {
        return Err(CliError::Usage(
            "install requires at least one ref (`<ns>/<name>@<version>`)".into(),
        ));
    }
    if args.refs.len() != 1 {
        return Err(CliError::Usage(
            "install accepts exactly one ref in v1 (multi-ref install will land alongside `add`)"
                .into(),
        ));
    }

    let registry_str = ctx.registry.clone().unwrap_or_else(|| DEFAULT_REGISTRY.into());
    let client = RegistryClient::from_env_str(&registry_str)?;
    let store = ctx.open_store()?;

    let r = parse_ref(&args.refs[0])?;
    let (namespace, name, version) = match r {
        Ref::Exact { namespace, name, version } => (namespace, name, version),
        Ref::Hash(_) => {
            return Err(CliError::Usage(
                "install requires <ns>/<name>@<version>; raw manifest hashes are not installable yet".into(),
            ));
        }
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio: {e}")))?;
    rt.block_on(fetch_into_store(&client, &store, &namespace, &name, &version))?;

    // After fetching, manifest, every layer blob, and the ref are all in
    // the local store. Resolve from the now-populated store and stack into
    // the output directory using the same fast-path stack uses.
    let root = build_root_ref(&namespace, &name, &version)?;
    let source = build_offline_source(&store)?;
    let resolution: Resolution = rt
        .block_on(resolve(&[root], &source, None, Some(&store)))
        .map_err(|e| CliError::Resolution(e.to_string()))?;

    if !resolution.fetch_plan.items.is_empty() {
        // Single-root install populated everything for this ref; if the
        // resolver still wants more, it's a transitive dep we don't yet
        // support in v1.
        return Err(CliError::Resolution(format!(
            "{} blob(s) still missing after install; transitive registry resolution not yet implemented",
            resolution.fetch_plan.items.len()
        )));
    }

    let out: Utf8PathBuf = args
        .out
        .clone()
        .unwrap_or_else(|| Utf8PathBuf::from("elu-out"));
    let hook_mode = match ctx.hooks {
        Some(CliHookMode::Off) => LayersHookMode::Off,
        _ => LayersHookMode::Safe,
    };
    let stats = layers_stack(&store, &resolution, &out, hook_mode, false)
        .map_err(|e| CliError::Generic(format!("stack: {e}")))?;

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "installed",
                "namespace": namespace,
                "name": name,
                "version": version,
                "out": out.to_string(),
                "layers": stats.layers,
                "entries_applied": stats.apply.entries_applied,
            }),
        );
    } else {
        println!(
            "installed {namespace}/{name}@{version} → {out} ({} layers, {} entries)",
            stats.layers, stats.apply.entries_applied
        );
    }
    Ok(())
}

/// Fetch the manifest and every layer blob for `ns/name@version` from the
/// registry into the local store. Records the ref. Idempotent: skips items
/// already present.
async fn fetch_into_store(
    client: &RegistryClient,
    store: &dyn Store,
    namespace: &str,
    name: &str,
    version: &str,
) -> Result<(), CliError> {
    let record: PackageRecord = client.fetch_package(namespace, name, version).await?;

    // 1. Manifest blob: fetch, verify, persist.
    let existing_manifest = store
        .get_manifest(&record.manifest_blob_id)
        .map_err(CliError::from)?;
    if existing_manifest.is_none() {
        let manifest_bytes = client.fetch_bytes(&record.manifest_url).await?;
        verify_manifest(&manifest_bytes, &record.manifest_blob_id)?;
        let stored_hash = store
            .put_manifest(&manifest_bytes)
            .map_err(CliError::from)?;
        if stored_hash != record.manifest_blob_id {
            return Err(CliError::Generic(format!(
                "manifest hash mismatch after store: expected {}, got {stored_hash}",
                record.manifest_blob_id,
            )));
        }
    }

    // 2. Layer blobs: fetch any missing, verify, put_blob (which re-derives
    //    the diff_id from the compressed bytes).
    for layer in &record.layers {
        if store.has(&layer.blob_id).map_err(CliError::from)? {
            continue;
        }
        let bytes = client.fetch_bytes(&layer.url).await?;
        verify_blob(&bytes, &layer.blob_id)?;
        let mut cursor = Cursor::new(&bytes);
        let put = store
            .put_blob(&mut cursor)
            .map_err(CliError::from)?;
        if put.blob_id != layer.blob_id {
            return Err(CliError::Generic(format!(
                "layer blob_id mismatch after store: expected {}, got {}",
                layer.blob_id, put.blob_id,
            )));
        }
        if put.diff_id != layer.diff_id {
            return Err(CliError::Generic(format!(
                "layer diff_id mismatch after store: expected {}, got {}",
                layer.diff_id, put.diff_id,
            )));
        }
    }

    // 3. Record the ref so the offline source can find it.
    store
        .put_ref(namespace, name, version, &record.manifest_blob_id)
        .map_err(CliError::from)?;

    Ok(())
}

fn build_root_ref(ns: &str, name: &str, version: &str) -> Result<RootRef, CliError> {
    let package: PackageRef = format!("{ns}/{name}")
        .parse()
        .map_err(CliError::Usage)?;
    let req = VersionReq::parse(&format!("={version}"))
        .map_err(|e| CliError::Usage(format!("version req: {e}")))?;
    Ok(RootRef {
        package,
        version: VersionSpec::Range(req),
    })
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
