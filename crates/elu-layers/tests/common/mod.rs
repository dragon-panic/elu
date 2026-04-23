#![allow(dead_code)]

use std::io::{Read, Write};

use camino::Utf8Path;
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::hash::DiffId;
use elu_store::store::Store;
use tempfile::TempDir;

pub struct Env {
    pub store_dir: TempDir,
    pub store: FsStore,
    pub work_dir: TempDir,
}

pub fn env() -> Env {
    let store_dir = TempDir::new().unwrap();
    let store_root = Utf8Path::from_path(store_dir.path()).unwrap();
    let store = FsStore::init_with_fsync(store_root, FsyncMode::Never).unwrap();
    let work_dir = TempDir::new().unwrap();
    Env {
        store_dir,
        store,
        work_dir,
    }
}

pub fn work(env: &Env) -> &Utf8Path {
    Utf8Path::from_path(env.work_dir.path()).unwrap()
}

pub struct Tar {
    builder: tar::Builder<Vec<u8>>,
}

impl Tar {
    pub fn new() -> Self {
        Self {
            builder: tar::Builder::new(Vec::new()),
        }
    }

    pub fn file(mut self, path: &str, content: &[u8]) -> Self {
        self.file_mode(path, content, 0o644);
        let _ = path;
        let _ = content;
        self
    }

    pub fn file_mode(&mut self, path: &str, content: &[u8], mode: u32) -> &mut Self {
        let mut header = tar::Header::new_gnu();
        header.set_path(path).unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(mode);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        self.builder.append(&header, content).unwrap();
        self
    }

    pub fn file_mode_owned(mut self, path: &str, content: &[u8], mode: u32) -> Self {
        self.file_mode(path, content, mode);
        self
    }

    pub fn dir(mut self, path: &str, mode: u32) -> Self {
        let mut header = tar::Header::new_gnu();
        let dir_path = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };
        header.set_path(&dir_path).unwrap();
        header.set_size(0);
        header.set_mode(mode);
        header.set_entry_type(tar::EntryType::Directory);
        header.set_cksum();
        self.builder.append(&header, &[][..]).unwrap();
        self
    }

    pub fn symlink(mut self, link_path: &str, target: &str) -> Self {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        self.builder
            .append_link(&mut header, link_path, target)
            .unwrap();
        self
    }

    pub fn raw_path(mut self, path: &str, content: &[u8]) -> Self {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        // Manually set the path bytes to allow `..` and absolute paths.
        let bytes = path.as_bytes();
        let len = bytes.len().min(100);
        header.as_old_mut().name[..len].copy_from_slice(&bytes[..len]);
        header.set_cksum();
        self.builder.append(&header, content).unwrap();
        self
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.builder.into_inner().unwrap()
    }
}

pub fn store_plain(env: &Env, tar_bytes: &[u8]) -> DiffId {
    env.store.put_blob(&mut &tar_bytes[..]).unwrap().diff_id
}

pub fn store_gzip(env: &Env, tar_bytes: &[u8]) -> DiffId {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(tar_bytes).unwrap();
    let gz = encoder.finish().unwrap();
    env.store.put_blob(&mut &gz[..]).unwrap().diff_id
}

pub fn store_zstd(env: &Env, tar_bytes: &[u8]) -> DiffId {
    let mut encoded = Vec::new();
    let mut encoder = zstd::stream::write::Encoder::new(&mut encoded, 0).unwrap();
    encoder.write_all(tar_bytes).unwrap();
    encoder.finish().unwrap();
    env.store.put_blob(&mut &encoded[..]).unwrap().diff_id
}

pub fn read_to_string(path: &Utf8Path) -> String {
    let mut buf = String::new();
    std::fs::File::open(path.as_std_path())
        .unwrap()
        .read_to_string(&mut buf)
        .unwrap();
    buf
}
