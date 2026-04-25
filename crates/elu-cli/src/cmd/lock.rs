//! `elu lock` — resolve the project manifest and write `./elu.lock`.
//!
//! Slice 1 (cx WKIW.wX0h.bP11) of the resolver-driven CLI surface arc.
//! Walks up from the cwd to find `elu.toml` (cargo's rule), runs the
//! resolver against an offline source built from the local store, and
//! writes the resulting lockfile next to the manifest. With
//! `--locked`, errors with exit 7 if the would-be lockfile differs
//! from what is already on disk.

use std::fs;

use elu_manifest::{Manifest, from_toml_str};
use elu_resolver::lockfile::{Lockfile, lock as resolver_lock};
use elu_resolver::source::OfflineSource;
use elu_store::store::{RefFilter, Store};

use crate::cli::LockArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::lockfile;
use crate::output::emit_event;

pub fn run(ctx: &GlobalCtx, _args: LockArgs) -> Result<(), CliError> {
    let project = lockfile::find_project_root_from_cwd()?;
    let manifest = read_manifest(&project.manifest_path())?;

    let store = ctx.open_store()?;
    let source = build_offline_source(&store)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio: {e}")))?;
    let new_lock: Lockfile = runtime
        .block_on(resolver_lock(&manifest, &source))
        .map_err(|e| CliError::Resolution(e.to_string()))?;

    let lockfile_path = project.lockfile_path();
    let existing = lockfile::read(&lockfile_path)?;

    let differs = match &existing {
        None => true,
        Some(disk) => disk != &new_lock,
    };

    if ctx.locked && differs {
        return Err(CliError::Lockfile(format!(
            "--locked: lockfile at {lockfile_path} would change",
        )));
    }

    if differs {
        lockfile::write(&lockfile_path, &new_lock)?;
    }

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "locked",
                "path": lockfile_path.to_string(),
                "packages": new_lock.packages.len(),
                "changed": differs,
            }),
        );
    } else if differs {
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

fn read_manifest(path: &camino::Utf8Path) -> Result<Manifest, CliError> {
    let s = fs::read_to_string(path)
        .map_err(|e| CliError::Usage(format!("read {path}: {e}")))?;
    from_toml_str(&s).map_err(CliError::from)
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
