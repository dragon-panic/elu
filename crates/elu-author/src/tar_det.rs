use std::fs;
use std::io::Read;

use camino::Utf8PathBuf;

use crate::report::{Diagnostic, ErrorCode};

#[derive(Clone, Debug)]
pub struct TarEntry {
    pub fs_path: Utf8PathBuf,
    pub layer_path: String,
    pub mode: Option<u32>,
}

impl TarEntry {
    pub fn file(fs_path: Utf8PathBuf, layer_path: String, mode: Option<u32>) -> Self {
        Self {
            fs_path,
            layer_path,
            mode,
        }
    }
}

pub fn build_deterministic_tar(entries: &[TarEntry]) -> Result<Vec<u8>, Diagnostic> {
    let mut sorted: Vec<&TarEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.layer_path.cmp(&b.layer_path));

    let buf: Vec<u8> = Vec::new();
    let mut builder = tar::Builder::new(buf);
    builder.mode(tar::HeaderMode::Deterministic);

    for e in sorted {
        let mut file = fs::File::open(e.fs_path.as_std_path()).map_err(|err| {
            Diagnostic::new(
                "",
                ErrorCode::FileNotReadable,
                format!("cannot read {}: {err}", e.fs_path),
            )
        })?;
        let metadata = file.metadata().map_err(|err| {
            Diagnostic::new(
                "",
                ErrorCode::FileNotReadable,
                format!("cannot stat {}: {err}", e.fs_path),
            )
        })?;

        let mut data = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut data).map_err(|err| {
            Diagnostic::new(
                "",
                ErrorCode::FileNotReadable,
                format!("read failed {}: {err}", e.fs_path),
            )
        })?;

        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(data.len() as u64);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        let mode = e.mode.unwrap_or(0o644);
        header.set_mode(mode);
        header.set_cksum();

        builder
            .append_data(&mut header, &e.layer_path, &data[..])
            .map_err(|err| {
                Diagnostic::new("", ErrorCode::FileNotReadable, format!("tar append: {err}"))
            })?;
    }

    builder
        .into_inner()
        .map_err(|err| Diagnostic::new("", ErrorCode::FileNotReadable, format!("tar finish: {err}")))
}
