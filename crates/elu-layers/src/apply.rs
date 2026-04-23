use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom};

use camino::{Utf8Path, Utf8PathBuf};
use tar::{Archive, Entry, EntryType};

use elu_store::hash::DiffId;
use elu_store::magic::{self, BlobEncoding};
use elu_store::store::Store;

use crate::error::LayerError;
use crate::whiteout::{self, Whiteout};

/// Counters reported by [`apply`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ApplyStats {
    pub entries_applied: u64,
    pub whiteouts: u64,
}

impl std::ops::AddAssign for ApplyStats {
    fn add_assign(&mut self, rhs: Self) {
        self.entries_applied += rhs.entries_applied;
        self.whiteouts += rhs.whiteouts;
    }
}

/// Materialize a single layer blob into `target`, with later-wins semantics
/// for overlapping paths. Whiteouts are consumed and never appear in output.
///
/// See `docs/prd/layers.md` § "Stacking Semantics".
pub fn apply(
    store: &dyn Store,
    diff_id: &DiffId,
    target: &Utf8Path,
) -> Result<ApplyStats, LayerError> {
    let blob_id = store
        .resolve_diff(diff_id)?
        .ok_or_else(|| LayerError::DiffNotFound(diff_id.clone()))?;
    let opaques = scan_opaques(store, &blob_id, diff_id)?;

    let mut file = store
        .open(&blob_id)?
        .ok_or_else(|| LayerError::DiffNotFound(diff_id.clone()))?;
    let encoding = sniff(&mut file)?;
    let reader = BufReader::new(file);
    match encoding {
        BlobEncoding::PlainTar => apply_archive(&mut Archive::new(reader), target, &opaques),
        BlobEncoding::Gzip => apply_archive(
            &mut Archive::new(flate2::read::GzDecoder::new(reader)),
            target,
            &opaques,
        ),
        BlobEncoding::Zstd => apply_archive(
            &mut Archive::new(zstd::stream::read::Decoder::new(reader)?),
            target,
            &opaques,
        ),
    }
}

/// First pass: collect parent directories that carry a `.wh..wh..opq` marker.
fn scan_opaques(
    store: &dyn Store,
    blob_id: &elu_store::hash::BlobId,
    diff_id: &DiffId,
) -> Result<HashSet<Utf8PathBuf>, LayerError> {
    let mut file = store
        .open(blob_id)?
        .ok_or_else(|| LayerError::DiffNotFound(diff_id.clone()))?;
    let encoding = sniff(&mut file)?;
    let reader = BufReader::new(file);
    let mut out = HashSet::new();
    match encoding {
        BlobEncoding::PlainTar => collect_opaques(&mut Archive::new(reader), &mut out)?,
        BlobEncoding::Gzip => collect_opaques(
            &mut Archive::new(flate2::read::GzDecoder::new(reader)),
            &mut out,
        )?,
        BlobEncoding::Zstd => collect_opaques(
            &mut Archive::new(zstd::stream::read::Decoder::new(reader)?),
            &mut out,
        )?,
    }
    Ok(out)
}

fn collect_opaques<R: Read>(
    archive: &mut Archive<R>,
    out: &mut HashSet<Utf8PathBuf>,
) -> Result<(), LayerError> {
    for entry in archive.entries()? {
        let entry = entry?;
        let path_bytes = entry.path_bytes();
        let path_str = match std::str::from_utf8(&path_bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(basename) = basename(path_str)
            && classify_basename(basename) == BasenameKind::Opaque
        {
            let parent = parent_of(path_str);
            out.insert(parent);
        }
    }
    Ok(())
}

fn sniff(file: &mut File) -> Result<BlobEncoding, LayerError> {
    let mut peek = [0u8; 512];
    let n = read_fill(file, &mut peek)?;
    file.seek(SeekFrom::Start(0))?;
    magic::sniff_encoding(&peek[..n]).ok_or(LayerError::UnknownEncoding)
}

fn read_fill<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

fn apply_archive<R: Read>(
    archive: &mut Archive<R>,
    target: &Utf8Path,
    opaques: &HashSet<Utf8PathBuf>,
) -> Result<ApplyStats, LayerError> {
    fs::create_dir_all(target.as_std_path())?;
    let mut stats = ApplyStats::default();
    let mut cleared: HashSet<Utf8PathBuf> = HashSet::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        apply_entry(&mut entry, target, opaques, &mut cleared, &mut stats)?;
    }
    Ok(stats)
}

fn apply_entry<R: Read>(
    entry: &mut Entry<'_, R>,
    target: &Utf8Path,
    opaques: &HashSet<Utf8PathBuf>,
    cleared: &mut HashSet<Utf8PathBuf>,
    stats: &mut ApplyStats,
) -> Result<(), LayerError> {
    let path_bytes = entry.path_bytes();
    let path_str = std::str::from_utf8(&path_bytes).map_err(|_| LayerError::NonUtf8Path)?;
    let rel = sanitize_rel(path_str)?;

    // Whiteout dispatch (consume, never materialize).
    if let Some(basename) = rel.file_name() {
        match whiteout::classify(basename) {
            Whiteout::Opaque => {
                let parent_rel = rel.parent().map(Utf8Path::to_path_buf).unwrap_or_default();
                clear_dir_once(target, &parent_rel, opaques, cleared)?;
                stats.whiteouts += 1;
                return Ok(());
            }
            Whiteout::Remove(name) => {
                let parent_rel = rel.parent().map(Utf8Path::to_path_buf).unwrap_or_default();
                let victim = target.join(&parent_rel).join(name);
                let _ = remove_existing(&victim);
                stats.whiteouts += 1;
                return Ok(());
            }
            Whiteout::None => {}
        }
    }

    let parent_rel = rel.parent().map(Utf8Path::to_path_buf).unwrap_or_default();
    clear_dir_once(target, &parent_rel, opaques, cleared)?;

    let dst = target.join(&rel);
    match entry.header().entry_type() {
        EntryType::Regular | EntryType::Continuous => {
            ensure_parent(&dst)?;
            write_regular(entry, &dst)?;
            stats.entries_applied += 1;
        }
        EntryType::Directory => {
            apply_directory(entry, &dst)?;
            stats.entries_applied += 1;
        }
        EntryType::Symlink => {
            ensure_parent(&dst)?;
            apply_symlink(entry, &dst)?;
            stats.entries_applied += 1;
        }
        _ => {
            // Other entry types: hardlinks, char/block devices, fifos.
            // PRD: "elu preserves [hardlinks] if the producer emits them but
            // does not require them." v1 treats hardlinks as regular files
            // (the unpack-as-plain-copy contract); other types are ignored.
            stats.entries_applied += 1;
        }
    }
    Ok(())
}

fn clear_dir_once(
    target: &Utf8Path,
    parent_rel: &Utf8Path,
    opaques: &HashSet<Utf8PathBuf>,
    cleared: &mut HashSet<Utf8PathBuf>,
) -> Result<(), LayerError> {
    let key = parent_rel.to_path_buf();
    if !opaques.contains(&key) || !cleared.insert(key) {
        return Ok(());
    }
    let dir = target.join(parent_rel);
    match fs::read_dir(dir.as_std_path()) {
        Ok(rd) => {
            for entry in rd {
                let entry = entry?;
                let p = entry.path();
                if entry.file_type()?.is_dir() {
                    fs::remove_dir_all(&p)?;
                } else {
                    fs::remove_file(&p)?;
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(LayerError::Io(e)),
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
enum BasenameKind {
    Opaque,
    Other,
}

fn classify_basename(b: &str) -> BasenameKind {
    if b == whiteout::OPAQUE_NAME {
        BasenameKind::Opaque
    } else {
        BasenameKind::Other
    }
}

fn basename(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit_once('/').map(|(_, b)| b).or(Some(trimmed))
}

fn parent_of(path: &str) -> Utf8PathBuf {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((parent, _)) => Utf8PathBuf::from(parent),
        None => Utf8PathBuf::new(),
    }
}

#[cfg(unix)]
fn apply_symlink<R: Read>(entry: &mut Entry<'_, R>, dst: &Utf8Path) -> Result<(), LayerError> {
    let target = entry.link_name()?.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "symlink entry missing link name")
    })?;
    remove_existing(dst)?;
    std::os::unix::fs::symlink(target.as_ref(), dst.as_std_path())?;
    Ok(())
}

#[cfg(not(unix))]
fn apply_symlink<R: Read>(_entry: &mut Entry<'_, R>, _dst: &Utf8Path) -> Result<(), LayerError> {
    Ok(())
}

fn remove_existing(path: &Utf8Path) -> Result<(), LayerError> {
    match fs::symlink_metadata(path.as_std_path()) {
        Ok(meta) => {
            if meta.is_dir() {
                fs::remove_dir_all(path.as_std_path())?;
            } else {
                fs::remove_file(path.as_std_path())?;
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(LayerError::Io(e)),
    }
}

fn apply_directory<R: Read>(entry: &mut Entry<'_, R>, dst: &Utf8Path) -> Result<(), LayerError> {
    let mode = entry.header().mode().unwrap_or(0o755);
    fs::create_dir_all(dst.as_std_path())?;
    set_mode(dst, mode)?;
    Ok(())
}

fn write_regular<R: Read>(entry: &mut Entry<'_, R>, dst: &Utf8Path) -> Result<(), LayerError> {
    use std::io::Write;

    let mode = entry.header().mode().unwrap_or(0o644);
    remove_existing(dst)?;
    let mut f = File::create(dst.as_std_path())?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = entry.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n])?;
    }
    set_mode(dst, mode)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Utf8Path, mode: u32) -> Result<(), LayerError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path.as_std_path())?.permissions();
    perms.set_mode(mode & 0o7777);
    fs::set_permissions(path.as_std_path(), perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Utf8Path, _mode: u32) -> Result<(), LayerError> {
    Ok(())
}

fn ensure_parent(dst: &Utf8Path) -> Result<(), LayerError> {
    if let Some(parent) = dst.parent()
        && !parent.as_str().is_empty()
    {
        fs::create_dir_all(parent.as_std_path())?;
    }
    Ok(())
}

/// Reject `..` components and absolute paths. Strip leading `./`.
fn sanitize_rel(path: &str) -> Result<Utf8PathBuf, LayerError> {
    let p = Utf8Path::new(path);
    if p.is_absolute() {
        return Err(LayerError::UnsafePath(path.to_string()));
    }
    let mut out = Utf8PathBuf::new();
    for comp in p.components() {
        match comp {
            camino::Utf8Component::Normal(s) => out.push(s),
            camino::Utf8Component::CurDir => {}
            camino::Utf8Component::ParentDir
            | camino::Utf8Component::RootDir
            | camino::Utf8Component::Prefix(_) => {
                return Err(LayerError::UnsafePath(path.to_string()));
            }
        }
    }
    Ok(out)
}
