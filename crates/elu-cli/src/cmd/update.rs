//! `elu update` — re-resolve manifest roots and rewrite `elu.lock`.
//! Slice 4 (cx WKIW.wX0h.jUbi).
//!
//! With no name args, re-resolves every dep ignoring lockfile pins.
//! With names, re-resolves only the named packages (and their
//! transitive deps) while keeping the rest pinned to the existing
//! lockfile entries. Does not stack. Does not mutate the manifest.

use std::fs;

use elu_manifest::{Manifest, PackageRef, from_toml_str};
use elu_resolver::lockfile::{Lockfile, lock as resolver_lock, update as resolver_update};
use elu_resolver::source::OfflineSource;
use elu_store::store::{RefFilter, Store};

use crate::cli::UpdateArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::lockfile;
use crate::output::emit_event;

pub fn run(ctx: &GlobalCtx, args: UpdateArgs) -> Result<(), CliError> {
    let project = lockfile::find_project_root_from_cwd()?;
    let manifest_path = project.manifest_path();
    let lockfile_path = project.lockfile_path();

    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|e| CliError::Usage(format!("read {manifest_path}: {e}")))?;
    let manifest: Manifest = from_toml_str(&manifest_text).map_err(CliError::from)?;

    // CLI accepts `<ns>/<name>` refs (per PRD examples). The resolver's
    // update API filters by post-slash name, so we extract those.
    let names: Vec<String> = args
        .names
        .iter()
        .map(|raw| {
            raw.parse::<PackageRef>()
                .map(|r| {
                    r.as_str()
                        .split_once('/')
                        .map(|(_, n)| n.to_string())
                        .unwrap_or_default()
                })
                .map_err(CliError::Usage)
        })
        .collect::<Result<_, _>>()?;

    let store = ctx.open_store()?;
    let source = build_offline_source(&store)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio: {e}")))?;

    let new_lock: Lockfile = if names.is_empty() {
        // No-name update == full relock ignoring pins.
        runtime
            .block_on(resolver_lock(&manifest, &source))
            .map_err(|e| CliError::Resolution(e.to_string()))?
    } else {
        let existing = lockfile::read(&lockfile_path)?.unwrap_or_default();
        runtime
            .block_on(resolver_update(&manifest, &existing, Some(&names), &source))
            .map_err(|e| CliError::Resolution(e.to_string()))?
    };

    let existing_lock = lockfile::read(&lockfile_path)?;
    let lock_changed = existing_lock.as_ref() != Some(&new_lock);

    if ctx.locked && lock_changed {
        return Err(CliError::Lockfile(format!(
            "--locked: `update` would change {lockfile_path}",
        )));
    }

    if lock_changed {
        lockfile::write(&lockfile_path, &new_lock)?;
    }

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "updated",
                "names": args.names,
                "lock_changed": lock_changed,
                "packages": new_lock.packages.len(),
            }),
        );
    } else if lock_changed {
        println!(
            "wrote {lockfile_path} ({} package{})",
            new_lock.packages.len(),
            if new_lock.packages.len() == 1 { "" } else { "s" },
        );
    } else {
        println!("{lockfile_path} up to date");
    }
    Ok(())
}

fn build_offline_source(store: &dyn Store) -> Result<OfflineSource, CliError> {
    let mut source = OfflineSource::new();
    for entry in store.list_refs(RefFilter::default())? {
        let bytes = store
            .get_manifest(&entry.hash)?
            .ok_or_else(|| CliError::Store(format!("manifest blob missing: {}", entry.hash)))?;
        let manifest = parse_stored_manifest(&bytes)?;
        source.insert(manifest, entry.hash);
    }
    Ok(source)
}

fn parse_stored_manifest(bytes: &[u8]) -> Result<Manifest, CliError> {
    if let Ok(m) = serde_json::from_slice::<Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes)
        .map_err(|_| CliError::Store("manifest is not utf-8".into()))?;
    from_toml_str(s).map_err(CliError::from)
}
