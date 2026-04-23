use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::OutputError;
use crate::{Compression, Outcome, TarOpts};

/// Materialize `staging` as a tar archive at `target`.
///
/// Entries are emitted in sorted path order. When `opts.deterministic` is
/// set, mtime and uid/gid are zeroed so identical inputs produce identical
/// bytes.
///
/// The archive is written to a `.tmp` sibling and renamed on success, so
/// a failure mid-write leaves no artifact at `target`.
pub fn materialize(
    staging: &Utf8Path,
    target: &Utf8Path,
    opts: &TarOpts,
) -> Result<Outcome, OutputError> {
    if !staging.as_std_path().is_dir() {
        return Err(OutputError::StagingNotDir(staging.to_path_buf()));
    }
    if target.as_std_path().exists() {
        if !opts.force {
            return Err(OutputError::TargetExists(target.to_path_buf()));
        }
        fs::remove_file(target.as_std_path()).or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(OutputError::Io(e))
            }
        })?;
    }
    if let Some(parent) = target.parent()
        && !parent.as_str().is_empty()
    {
        fs::create_dir_all(parent.as_std_path())?;
    }

    let tmp = tmp_sibling(target);
    let write_result = (|| -> Result<u64, OutputError> {
        let file = fs::File::create(tmp.as_std_path())?;
        let mut writer = wrap_compression(Box::new(file), opts.compress, opts.level)?;
        write_archive(staging, &mut writer, opts.deterministic)?;
        writer.flush()?;
        drop(writer);
        let size = fs::metadata(tmp.as_std_path())?.len();
        Ok(size)
    })();

    match write_result {
        Ok(bytes) => {
            fs::rename(tmp.as_std_path(), target.as_std_path())?;
            cleanup_staging(staging)?;
            Ok(Outcome { bytes })
        }
        Err(e) => {
            let _ = fs::remove_file(tmp.as_std_path());
            Err(e)
        }
    }
}

fn tmp_sibling(target: &Utf8Path) -> Utf8PathBuf {
    let name = target.file_name().unwrap_or("elu-out");
    let parent = target
        .parent()
        .filter(|p| !p.as_str().is_empty())
        .map(Utf8Path::to_path_buf)
        .unwrap_or_else(|| Utf8PathBuf::from("."));
    parent.join(format!(".{name}.tmp"))
}

fn cleanup_staging(staging: &Utf8Path) -> Result<(), OutputError> {
    fs::remove_dir_all(staging.as_std_path())?;
    Ok(())
}

/// Write a tar archive from `staging` into `out`.
fn write_archive<W: Write>(
    staging: &Utf8Path,
    out: &mut W,
    deterministic: bool,
) -> Result<(), OutputError> {
    let entries = collect_sorted(staging)?;
    let mut builder = tar::Builder::new(out);
    builder.mode(tar::HeaderMode::Deterministic);
    for (rel, kind) in entries {
        write_entry(&mut builder, staging, &rel, kind, deterministic)?;
    }
    builder.finish()?;
    Ok(())
}

#[derive(Clone, Copy)]
enum EntryKind {
    Dir,
    File,
    Symlink,
}

fn collect_sorted(staging: &Utf8Path) -> Result<BTreeMap<String, EntryKind>, OutputError> {
    let mut out = BTreeMap::new();
    walk(staging, &mut out, Utf8Path::new(""))?;
    Ok(out)
}

fn walk(
    abs: &Utf8Path,
    out: &mut BTreeMap<String, EntryKind>,
    rel: &Utf8Path,
) -> Result<(), OutputError> {
    for entry in fs::read_dir(abs.as_std_path())? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let ft = entry.file_type()?;
        let child_rel: Utf8PathBuf = if rel.as_str().is_empty() {
            Utf8PathBuf::from(name_str.as_ref())
        } else {
            rel.join(name_str.as_ref())
        };
        let child_abs = abs.join(name_str.as_ref());
        if ft.is_symlink() {
            out.insert(child_rel.as_str().to_string(), EntryKind::Symlink);
        } else if ft.is_dir() {
            out.insert(child_rel.as_str().to_string(), EntryKind::Dir);
            walk(&child_abs, out, &child_rel)?;
        } else if ft.is_file() {
            out.insert(child_rel.as_str().to_string(), EntryKind::File);
        }
    }
    Ok(())
}

fn write_entry<W: Write>(
    builder: &mut tar::Builder<W>,
    root: &Utf8Path,
    rel: &str,
    kind: EntryKind,
    deterministic: bool,
) -> Result<(), OutputError> {
    let abs = root.join(rel);
    let meta = fs::symlink_metadata(abs.as_std_path())?;
    let mut header = tar::Header::new_ustar();
    #[cfg(unix)]
    {
        header.set_mode(meta.mode() & 0o7777);
    }
    #[cfg(not(unix))]
    {
        header.set_mode(0o644);
    }
    if deterministic {
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
    } else {
        #[cfg(unix)]
        {
            let mtime = meta.mtime();
            header.set_mtime(if mtime < 0 { 0 } else { mtime as u64 });
            header.set_uid(meta.uid() as u64);
            header.set_gid(meta.gid() as u64);
        }
    }

    let tar_path = match kind {
        EntryKind::Dir => {
            if rel.ends_with('/') {
                rel.to_string()
            } else {
                format!("{rel}/")
            }
        }
        _ => rel.to_string(),
    };

    match kind {
        EntryKind::Dir => {
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header
                .set_path(&tar_path)
                .map_err(|e| OutputError::Io(io::Error::other(format!("tar path: {e}"))))?;
            header.set_cksum();
            builder.append(&header, io::empty())?;
        }
        EntryKind::Symlink => {
            let link_target = fs::read_link(abs.as_std_path())?;
            let link_target_str = link_target
                .to_str()
                .ok_or_else(|| OutputError::Io(io::Error::other("symlink target not utf-8")))?;
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header
                .set_path(&tar_path)
                .map_err(|e| OutputError::Io(io::Error::other(format!("tar path: {e}"))))?;
            header
                .set_link_name(link_target_str)
                .map_err(|e| OutputError::Io(io::Error::other(format!("tar link: {e}"))))?;
            header.set_cksum();
            builder.append(&header, io::empty())?;
        }
        EntryKind::File => {
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(meta.len());
            header
                .set_path(&tar_path)
                .map_err(|e| OutputError::Io(io::Error::other(format!("tar path: {e}"))))?;
            header.set_cksum();
            let file = fs::File::open(abs.as_std_path())?;
            builder.append(&header, file)?;
        }
    }
    Ok(())
}

fn wrap_compression(
    inner: Box<dyn Write>,
    compress: Compression,
    level: Option<i32>,
) -> Result<Box<dyn Write>, OutputError> {
    match compress {
        Compression::None => Ok(inner),
        Compression::Gzip => {
            let lvl = match level {
                Some(l) => flate2::Compression::new(l.clamp(0, 9) as u32),
                None => flate2::Compression::default(),
            };
            Ok(Box::new(flate2::write::GzEncoder::new(inner, lvl)))
        }
        Compression::Zstd => {
            let lvl = level.unwrap_or(0);
            let enc = zstd::stream::write::Encoder::new(inner, lvl)
                .map_err(OutputError::Io)?
                .auto_finish();
            Ok(Box::new(enc))
        }
        Compression::Xz => {
            let lvl = level.map(|l| l.clamp(0, 9) as u32).unwrap_or(6);
            Ok(Box::new(xz2::write::XzEncoder::new(inner, lvl)))
        }
    }
}
