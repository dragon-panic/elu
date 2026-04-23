use std::fs;
use std::io;

use camino::{Utf8Path, Utf8PathBuf};

use elu_hooks::{HookMode, HookRunner, HookStats, PackageContext};
use elu_resolver::Resolution;
use elu_store::hash::DiffId;
use elu_store::store::Store;

use crate::apply::{ApplyStats, apply};
use crate::error::LayerError;

/// Counters reported by [`stack`].
#[derive(Debug, Default)]
pub struct StackStats {
    pub layers: u64,
    pub apply: ApplyStats,
    pub hook: HookStats,
}

/// The ordered, deduplicated `diff_id` list to feed the stacker. Returns the
/// resolver's [`Resolution::layers`] verbatim — DFS, first-position dedup.
///
/// See `docs/prd/layers.md` § "Interface Sketch".
pub fn flatten(resolution: &Resolution) -> &[DiffId] {
    &resolution.layers
}

/// Apply each layer from `resolution` into a fresh staging directory rooted
/// under `parent_dir`, then run the post-unpack hook. On any error the
/// staging directory is cleaned up and the error is returned.
///
/// The returned [`Staging`] is an RAII handle: dropping it deletes the
/// staging tree. Call [`Staging::into_path`] to disarm the drop guard and
/// take ownership of the staging path (the caller then owns cleanup).
///
/// `stage` never mutates a final target; it is the building block for
/// materialization.
pub fn stage(
    store: &dyn Store,
    resolution: &Resolution,
    parent_dir: &Utf8Path,
    hook_mode: HookMode,
) -> Result<(Staging, StackStats), LayerError> {
    let staging = Staging::create(parent_dir)?;
    let mut stats = StackStats::default();
    for diff_id in &resolution.layers {
        let s = apply(store, diff_id, staging.path())?;
        stats.apply += s;
        stats.layers += 1;
    }
    // PRD: hook runs once, after all layers applied, before finalize.
    // Hook ops come from the root manifest (the entry the resolver was
    // asked about), which is the first ResolvedManifest by convention.
    if let Some(root) = resolution.manifests.first()
        && !root.manifest.hook.ops.is_empty()
    {
        let pkg = root.package.as_str();
        let (ns, name) = pkg.split_once('/').unwrap_or(("", pkg));
        let version = root.manifest.package.version.to_string();
        let pkg_ctx = PackageContext {
            namespace: ns,
            name,
            version: &version,
            kind: &root.manifest.package.kind,
        };
        let runner = HookRunner::new(staging.path(), &pkg_ctx, hook_mode);
        stats.hook = runner.run(&root.manifest.hook.ops)?;
    }
    Ok((staging, stats))
}

/// Apply each layer from `resolution` into `target` in manifest order, run
/// the post-unpack hook, then atomically rename the staged tree into place.
/// On any error the staging directory is cleaned and `target` is left
/// untouched.
///
/// `force` removes a pre-existing target before staging; without it,
/// pre-existing targets fail with [`LayerError::TargetExists`].
///
/// See `docs/prd/layers.md` § "Stacking Semantics" and § "Post-Unpack Hook".
pub fn stack(
    store: &dyn Store,
    resolution: &Resolution,
    target: &Utf8Path,
    hook_mode: HookMode,
    force: bool,
) -> Result<StackStats, LayerError> {
    if target.as_std_path().exists() {
        if !force {
            return Err(LayerError::TargetExists(target.to_path_buf()));
        }
        remove_target(target)?;
    }

    let parent = target_parent(target);
    fs::create_dir_all(parent.as_std_path()).map_err(LayerError::Staging)?;
    let (staging, stats) = stage(store, resolution, &parent, hook_mode)?;
    staging.finalize(target)?;
    Ok(stats)
}

/// RAII handle for a staging directory. Dropping cleans the tree; call
/// [`Staging::into_path`] to take ownership (cleanup is the caller's job).
pub struct Staging {
    path: Utf8PathBuf,
    armed: bool,
}

impl Staging {
    fn create(parent: &Utf8Path) -> Result<Self, LayerError> {
        let parent = if parent.as_str().is_empty() {
            Utf8PathBuf::from(".")
        } else {
            parent.to_path_buf()
        };
        fs::create_dir_all(parent.as_std_path()).map_err(LayerError::Staging)?;
        let tmp = tempfile::Builder::new()
            .prefix(".elu-stage.")
            .tempdir_in(parent.as_std_path())
            .map_err(LayerError::Staging)?;
        let path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf())
            .map_err(|p| LayerError::Staging(io::Error::other(format!("staging not utf8: {p:?}"))))?;
        // Detach: we manage cleanup ourselves so we can rename on success.
        std::mem::forget(tmp);
        Ok(Staging { path, armed: true })
    }

    /// Path to the staging root.
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Disarm the drop guard and return the staging path. Caller owns cleanup.
    pub fn into_path(mut self) -> Utf8PathBuf {
        self.armed = false;
        std::mem::take(&mut self.path)
    }

    /// Rename the staging directory onto `target`. Staging must be on the
    /// same filesystem as `target`'s parent.
    pub fn finalize(mut self, target: &Utf8Path) -> Result<(), LayerError> {
        fs::rename(self.path.as_std_path(), target.as_std_path())?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(self.path.as_std_path());
        }
    }
}

fn target_parent(target: &Utf8Path) -> Utf8PathBuf {
    match target.parent() {
        Some(p) if !p.as_str().is_empty() => p.to_path_buf(),
        _ => Utf8PathBuf::from("."),
    }
}

fn remove_target(target: &Utf8Path) -> Result<(), LayerError> {
    match fs::symlink_metadata(target.as_std_path()) {
        Ok(meta) => {
            if meta.is_dir() {
                fs::remove_dir_all(target.as_std_path())?;
            } else {
                fs::remove_file(target.as_std_path())?;
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(LayerError::Io(e)),
    }
}
