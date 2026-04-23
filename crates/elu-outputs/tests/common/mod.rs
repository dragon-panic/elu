#![allow(dead_code)]

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

/// Create a populated staging tree under `parent` with a handful of files,
/// including nested dirs. Returns the staging path (caller owns cleanup via
/// the enclosing TempDir).
pub fn populate_staging(parent: &Utf8Path, name: &str) -> Utf8PathBuf {
    let staging = parent.join(name);
    fs::create_dir_all(staging.as_std_path()).unwrap();
    fs::write(staging.join("root.txt").as_std_path(), b"root").unwrap();
    fs::create_dir_all(staging.join("sub").as_std_path()).unwrap();
    fs::write(staging.join("sub/inner.txt").as_std_path(), b"inner").unwrap();
    staging
}

pub fn read(path: &Utf8Path) -> String {
    String::from_utf8(fs::read(path.as_std_path()).unwrap()).unwrap()
}
