//! `elu add` — append (or update) dependencies in the project's
//! `elu.toml` and refresh `elu.lock`. Slice 2 (cx WKIW.wX0h.BfF6).
//!
//! Atomic w.r.t. the manifest: if resolution fails the on-disk
//! manifest is not mutated. Idempotent: re-adding the same ref with
//! the same version spec is a no-op. A different version spec for an
//! already-present ref overwrites it (cargo's behavior).

use std::fs;

use elu_manifest::{Dependency, Manifest, PackageRef, VersionSpec, from_toml_str, to_toml_string};
use elu_resolver::lockfile::{Lockfile, lock as resolver_lock};
use elu_resolver::source::OfflineSource;
use elu_store::store::{RefFilter, Store};

use crate::cli::AddArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::lockfile;
use crate::output::emit_event;
use crate::refs_parse::parse_dep_spec;

pub fn run(ctx: &GlobalCtx, args: AddArgs) -> Result<(), CliError> {
    let project = lockfile::find_project_root_from_cwd()?;
    let manifest_path = project.manifest_path();
    let lockfile_path = project.lockfile_path();

    let original_text = fs::read_to_string(&manifest_path)
        .map_err(|e| CliError::Usage(format!("read {manifest_path}: {e}")))?;
    let mut manifest = from_toml_str(&original_text).map_err(CliError::from)?;

    let mut manifest_changed = false;
    for raw in &args.refs {
        let (reference, version) = parse_dep_spec(raw)?;
        if upsert_dep(&mut manifest, reference, version) {
            manifest_changed = true;
        }
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

    if ctx.locked && (manifest_changed || lock_changed) {
        return Err(CliError::Lockfile(format!(
            "--locked: `add` would change {}",
            change_summary(manifest_changed, lock_changed),
        )));
    }

    if manifest_changed {
        let new_text = to_toml_string(&manifest).map_err(CliError::from)?;
        atomic_write(&manifest_path, new_text.as_bytes())?;
    }
    if lock_changed {
        lockfile::write(&lockfile_path, &new_lock)?;
    }

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "added",
                "refs": args.refs,
                "manifest_changed": manifest_changed,
                "lock_changed": lock_changed,
                "packages": new_lock.packages.len(),
            }),
        );
    } else if manifest_changed || lock_changed {
        let added: Vec<&str> = args.refs.iter().map(String::as_str).collect();
        println!("added {} → {manifest_path}", added.join(", "));
    } else {
        println!("{manifest_path} unchanged");
    }
    Ok(())
}

/// Insert or update a dep on `manifest`. Returns true if `manifest`
/// actually changed.
fn upsert_dep(manifest: &mut Manifest, reference: PackageRef, version: VersionSpec) -> bool {
    if let Some(existing) = manifest
        .dependencies
        .iter_mut()
        .find(|d| d.reference == reference)
    {
        if existing.version == version {
            return false;
        }
        existing.version = version;
        return true;
    }
    manifest.dependencies.push(Dependency { reference, version });
    true
}

fn change_summary(manifest_changed: bool, lock_changed: bool) -> &'static str {
    match (manifest_changed, lock_changed) {
        (true, true) => "manifest and lockfile",
        (true, false) => "manifest",
        (false, true) => "lockfile",
        (false, false) => "nothing",
    }
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
