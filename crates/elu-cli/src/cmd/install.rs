//! `elu install <ref>...` — resolve a set of refs against a hybrid
//! (store + registry) source, fetch the full closure into the local
//! store, and stack into `--out` (default `./elu-out`).
//!
//! Slice 5 of the resolver-driven CLI surface arc (cx WKIW.MqEx).
//! Replaces the v1 single-ref-with-no-deps implementation.

use std::io::Cursor;
use std::sync::Arc;

use camino::Utf8PathBuf;
use elu_hooks::HookMode as LayersHookMode;
use elu_manifest::Manifest;
use elu_registry::client::fallback::RegistryClient;
use elu_registry::client::verify::{verify_blob, verify_manifest};
use elu_registry::source::RegistrySource;
use elu_resolver::source::OfflineSource;
use elu_resolver::types::{FetchKind, Resolution};
use elu_resolver::{RootRef, resolve};
use elu_stacker::stack as layers_stack;
use elu_store::store::{RefFilter, Store};

use crate::cli::{HookMode as CliHookMode, InstallArgs};
use crate::error::CliError;
use crate::global::{DEFAULT_REGISTRY, GlobalCtx};
use crate::lockfile;
use crate::output::emit_event;
use crate::refs_parse::parse_dep_spec;
use crate::source::HybridSource;

pub fn run(ctx: &GlobalCtx, args: InstallArgs) -> Result<(), CliError> {
    if args.refs.is_empty() {
        return Err(CliError::Usage(
            "install requires at least one ref (`<ns>/<name>[@<version>]`)".into(),
        ));
    }
    if ctx.offline {
        return Err(CliError::Network(
            "--offline forbids registry contact (install needs to fetch)".into(),
        ));
    }

    let registry_str = ctx.registry.clone().unwrap_or_else(|| DEFAULT_REGISTRY.into());
    let client = Arc::new(RegistryClient::from_env_str(&registry_str)?);
    let store = ctx.open_store()?;

    let roots: Vec<RootRef> = args
        .refs
        .iter()
        .map(|raw| {
            let (package, version) = parse_dep_spec(raw)?;
            Ok(RootRef { package, version })
        })
        .collect::<Result<Vec<_>, CliError>>()?;

    let registry_source = Arc::new(RegistrySource::new(client.clone()));
    let offline = build_offline_source(&store)?;
    let source = HybridSource::new(offline, Some(registry_source.clone()));

    // Read elu.lock if it exists next to elu.toml; absence is fine.
    let lockfile_on_disk = match lockfile::find_project_root_from_cwd() {
        Ok(project) => lockfile::read(&project.lockfile_path())?,
        Err(_) => None,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio: {e}")))?;
    let resolution: Resolution = rt
        .block_on(resolve(&roots, &source, lockfile_on_disk.as_ref(), Some(&store)))
        .map_err(|e| CliError::Resolution(e.to_string()))?;

    if ctx.locked
        && let Some(diff) = lockfile_diff(lockfile_on_disk.as_ref(), &resolution)
    {
        return Err(CliError::Lockfile(format!(
            "--locked: install would change the lockfile ({diff})",
        )));
    }

    rt.block_on(execute_fetch_plan(
        &client,
        &registry_source,
        &store,
        &resolution,
    ))?;

    // After fetching, persist refs for every resolved manifest so future
    // resolves from this store can serve them offline.
    for m in &resolution.manifests {
        let (ns, name) = split_pkg_str(&m.package);
        let v = m.manifest.package.version.to_string();
        store
            .put_ref(ns, name, &v, &m.hash)
            .map_err(CliError::from)?;
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
                "refs": args.refs,
                "packages": resolution.manifests.len(),
                "out": out.to_string(),
                "layers": stats.layers,
                "entries_applied": stats.apply.entries_applied,
            }),
        );
    } else {
        let names: Vec<String> = resolution
            .manifests
            .iter()
            .map(|m| {
                format!(
                    "{}/{}@{}",
                    m.manifest.package.namespace,
                    m.manifest.package.name,
                    m.manifest.package.version
                )
            })
            .collect();
        println!(
            "installed {} → {out} ({} layers, {} entries)",
            names.join(", "),
            stats.layers,
            stats.apply.entries_applied
        );
    }
    Ok(())
}

/// Walk the resolver's fetch plan and pull every missing blob into `store`.
/// For manifests we have the URL on the FetchItem (resolver populated it from
/// the registry source); for layers we look up the URL via the registry
/// source's per-diff_id cache.
async fn execute_fetch_plan(
    client: &RegistryClient,
    registry_source: &Arc<RegistrySource>,
    store: &dyn Store,
    resolution: &Resolution,
) -> Result<(), CliError> {
    for item in &resolution.fetch_plan.items {
        match &item.kind {
            FetchKind::Manifest(hash) => {
                if store.get_manifest(hash).map_err(CliError::from)?.is_some() {
                    continue;
                }
                let url = item
                    .url
                    .clone()
                    .or_else(|| registry_source.manifest_url_for_hash(hash))
                    .ok_or_else(|| {
                        CliError::Resolution(format!(
                            "no download URL for manifest {hash} (resolver source did not record one)",
                        ))
                    })?;
                let bytes = client.fetch_bytes(&url).await?;
                verify_manifest(&bytes, hash)?;
                let stored = store.put_manifest(&bytes).map_err(CliError::from)?;
                if stored != *hash {
                    return Err(CliError::Generic(format!(
                        "manifest hash mismatch after store: expected {hash}, got {stored}",
                    )));
                }
            }
            FetchKind::Layer(diff_id) => {
                if store.has_diff(diff_id).map_err(CliError::from)? {
                    continue;
                }
                let layer = registry_source.layer_record_for_diff(diff_id).ok_or_else(|| {
                    CliError::Resolution(format!(
                        "no layer record for {diff_id}; resolver fetched a manifest but registry source did not cache it",
                    ))
                })?;
                let bytes = client.fetch_bytes(&layer.url).await?;
                verify_blob(&bytes, &layer.blob_id)?;
                let mut cursor = Cursor::new(&bytes);
                let put = store.put_blob(&mut cursor).map_err(CliError::from)?;
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
        }
    }
    Ok(())
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

fn parse_manifest(bytes: &[u8]) -> Result<Manifest, CliError> {
    if let Ok(m) = serde_json::from_slice::<Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes)
        .map_err(|_| CliError::Store("manifest is not utf-8".into()))?;
    elu_manifest::from_toml_str(s).map_err(CliError::from)
}

fn split_pkg_str(p: &elu_manifest::types::PackageRef) -> (&str, &str) {
    p.as_str().split_once('/').expect("PackageRef invariant: contains '/'")
}

/// If `--locked` would refuse this resolution, return a short human-readable
/// reason; `None` means the resolution matches the lockfile exactly.
/// Refuse if any resolved manifest is absent from the lockfile, or if its
/// hash differs from the lockfile entry. (Extra entries in the lockfile that
/// the resolution doesn't touch are fine — they may belong to other roots.)
fn lockfile_diff(
    lockfile: Option<&elu_resolver::lockfile::Lockfile>,
    resolution: &Resolution,
) -> Option<String> {
    let Some(lock) = lockfile else {
        if resolution.manifests.is_empty() {
            return None;
        }
        return Some(format!(
            "no lockfile on disk; resolution introduces {} pin(s)",
            resolution.manifests.len()
        ));
    };
    for m in &resolution.manifests {
        let (ns, name) = split_pkg_str(&m.package);
        match lock.lookup(ns, name) {
            None => {
                return Some(format!(
                    "{ns}/{name}@{} is a new pin not in the lockfile",
                    m.manifest.package.version
                ));
            }
            Some(entry) => {
                if entry.hash != m.hash {
                    return Some(format!(
                        "{ns}/{name} hash differs: lockfile {} vs resolution {}",
                        entry.hash, m.hash
                    ));
                }
            }
        }
    }
    None
}
