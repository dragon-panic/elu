use camino::Utf8Path;

use std::fs;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FsyncMode {
    Always,
    Never,
}

pub fn atomic_write(path: &Utf8Path, data: &[u8], fsync: FsyncMode) -> Result<(), std::io::Error> {
    let parent = path.parent().expect("path must have a parent");
    fs::create_dir_all(parent)?;
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    fs::write(tmp.path(), data)?;
    if fsync == FsyncMode::Always {
        let f = fs::File::open(tmp.path())?;
        f.sync_data()?;
    }
    tmp.persist(path).map_err(|e| e.error)?;
    if fsync == FsyncMode::Always {
        sync_parent_dir(path)?;
    }
    Ok(())
}

pub fn sync_parent_dir(path: &Utf8Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        let dir = fs::File::open(parent.as_std_path())?;
        dir.sync_data()?;
    }
    Ok(())
}
