//! `elu remove` — strip a dependency from the project's `elu.toml`
//! and refresh `elu.lock`. Slice 3 (cx WKIW.wX0h.cXJm).
//!
//! Mirror of `add`: read manifest, drop the dep, resolve, atomically
//! write manifest + lockfile. Removing a non-present dep is a usage
//! error (exit 2) — cargo's behavior.

use std::fs;

use elu_manifest::{Manifest, PackageRef, from_toml_str, to_toml_string};
use elu_resolver::lockfile::{Lockfile, lock as resolver_lock};
use elu_resolver::source::OfflineSource;
use elu_store::store::{RefFilter, Store};

use crate::cli::RemoveArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::lockfile;
use crate::output::emit_event;

pub fn run(ctx: &GlobalCtx, args: RemoveArgs) -> Result<(), CliError> {
    let project = lockfile::find_project_root_from_cwd()?;
    let manifest_path = project.manifest_path();
    let lockfile_path = project.lockfile_path();

    let original_text = fs::read_to_string(&manifest_path)
        .map_err(|e| CliError::Usage(format!("read {manifest_path}: {e}")))?;
    let mut manifest = from_toml_str(&original_text).map_err(CliError::from)?;

    let target: PackageRef = args.name.parse().map_err(CliError::Usage)?;
    let original_len = manifest.dependencies.len();
    manifest.dependencies.retain(|d| d.reference != target);
    if manifest.dependencies.len() == original_len {
        return Err(CliError::Usage(format!(
            "no dependency `{target}` in {manifest_path}",
        )));
    }

    let store = ctx.open_store()?;
    let source = build_offline_source(&store)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio: {e}")))?;
    let new_lock: Lockfile = runtime
        .block_on(resolver_lock(&manifest, &source))
        .map_err(|e| CliError::Resolution(e.to_string()))?;

    let existing_lock = lockfile::read(&lockfile_path)?;
    let lock_changed = existing_lock.as_ref() != Some(&new_lock);

    if ctx.locked {
        return Err(CliError::Lockfile(format!(
            "--locked: `remove` would change manifest{}",
            if lock_changed { " and lockfile" } else { "" },
        )));
    }

    let new_text = to_toml_string(&manifest).map_err(CliError::from)?;
    atomic_write(&manifest_path, new_text.as_bytes())?;
    if lock_changed {
        lockfile::write(&lockfile_path, &new_lock)?;
    }

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "removed",
                "ref": target.to_string(),
                "lock_changed": lock_changed,
                "packages": new_lock.packages.len(),
            }),
        );
    } else {
        println!("removed {target} from {manifest_path}");
    }
    Ok(())
}

fn atomic_write(path: &camino::Utf8Path, bytes: &[u8]) -> Result<(), CliError> {
    use std::io::Write;
    let tmp = path.with_extension("toml.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| CliError::Generic(format!("open {tmp}: {e}")))?;
        f.write_all(bytes)
            .map_err(|e| CliError::Generic(format!("write {tmp}: {e}")))?;
        f.sync_all()
            .map_err(|e| CliError::Generic(format!("fsync {tmp}: {e}")))?;
    }
    fs::rename(&tmp, path)
        .map_err(|e| CliError::Generic(format!("rename {tmp} -> {path}: {e}")))?;
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
