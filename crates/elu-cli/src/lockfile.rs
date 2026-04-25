//! Project root discovery + lockfile read/write helpers used by the
//! resolver-driven CLI verbs (`lock`, `add`, `remove`, `update`,
//! `install`, `stack`).
//!
//! The discovery rule mirrors cargo: walk up from the current
//! directory until an `elu.toml` is found. The lockfile lives next
//! to that manifest. Running `elu lock` from a subdirectory
//! therefore updates the project-root lockfile, not a stray subdir
//! file. Spec: docs/prd/cli.md (`elu lock`), docs/prd/resolver.md.

use std::fs;
use std::io::Write;

use camino::{Utf8Path, Utf8PathBuf};
use elu_resolver::lockfile::Lockfile;

use crate::error::CliError;

const MANIFEST_NAME: &str = "elu.toml";
const LOCKFILE_NAME: &str = "elu.lock";

/// A located project root: the directory containing `elu.toml`.
#[derive(Debug, Clone)]
pub struct ProjectRoot {
    pub dir: Utf8PathBuf,
}

impl ProjectRoot {
    pub fn manifest_path(&self) -> Utf8PathBuf {
        self.dir.join(MANIFEST_NAME)
    }

    pub fn lockfile_path(&self) -> Utf8PathBuf {
        self.dir.join(LOCKFILE_NAME)
    }
}

/// Walk up from `start` looking for `elu.toml`. Returns the directory
/// that contains it. Errors with [`CliError::Usage`] (exit 2) when no
/// ancestor has one — same code cargo uses for "could not find
/// `Cargo.toml`".
pub fn find_project_root(start: &Utf8Path) -> Result<ProjectRoot, CliError> {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join(MANIFEST_NAME).is_file() {
            return Ok(ProjectRoot { dir: cur });
        }
        match cur.parent() {
            Some(parent) => cur = parent.to_path_buf(),
            None => {
                return Err(CliError::Usage(format!(
                    "could not find `{MANIFEST_NAME}` in `{start}` or any parent directory",
                )));
            }
        }
    }
}

/// Walk up from the current working directory.
pub fn find_project_root_from_cwd() -> Result<ProjectRoot, CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::Generic(format!("read cwd: {e}")))?;
    let cwd = Utf8PathBuf::from_path_buf(cwd)
        .map_err(|p| CliError::Generic(format!("cwd not utf-8: {}", p.display())))?;
    find_project_root(&cwd)
}

/// Read `elu.lock` if present. Missing file → `Ok(None)`. Decode
/// errors surface as [`CliError::Lockfile`] (exit 7).
pub fn read(path: &Utf8Path) -> Result<Option<Lockfile>, CliError> {
    match fs::read_to_string(path) {
        Ok(s) => Lockfile::from_toml_str(&s)
            .map(Some)
            .map_err(|e| CliError::Lockfile(format!("decode {path}: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CliError::Generic(format!("read {path}: {e}"))),
    }
}

/// Atomically replace `path` with the serialized lockfile (write
/// `<path>.tmp`, fsync, rename).
pub fn write(path: &Utf8Path, lockfile: &Lockfile) -> Result<(), CliError> {
    let body = lockfile
        .to_toml_string()
        .map_err(|e| CliError::Lockfile(format!("encode: {e}")))?;
    let tmp = path.with_extension("lock.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| CliError::Generic(format!("open {tmp}: {e}")))?;
        f.write_all(body.as_bytes())
            .map_err(|e| CliError::Generic(format!("write {tmp}: {e}")))?;
        f.sync_all()
            .map_err(|e| CliError::Generic(format!("fsync {tmp}: {e}")))?;
    }
    fs::rename(&tmp, path)
        .map_err(|e| CliError::Generic(format!("rename {tmp} -> {path}: {e}")))?;
    Ok(())
}
