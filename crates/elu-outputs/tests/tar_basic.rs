mod common;

use std::collections::BTreeMap;
use std::io::Read;

use camino::Utf8Path;
use elu_outputs::{TarOpts, tar as tar_out};
use tempfile::TempDir;

use common::populate_staging;

fn extract_tar(path: &Utf8Path) -> BTreeMap<String, Vec<u8>> {
    let file = std::fs::File::open(path.as_std_path()).unwrap();
    let mut archive = tar::Archive::new(file);
    let mut out = BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().into_owned();
        let header = entry.header();
        match header.entry_type() {
            tar::EntryType::Regular => {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf).unwrap();
                out.insert(path, buf);
            }
            tar::EntryType::Directory => {
                out.insert(path, Vec::new());
            }
            _ => {}
        }
    }
    out
}

#[test]
fn tar_basic_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join("out.tar");

    let out = tar_out::materialize(&staging, &target, &TarOpts::default()).unwrap();

    assert!(target.as_std_path().is_file());
    assert!(!staging.as_std_path().exists(), "staging cleaned up");
    assert!(out.bytes > 0);

    let entries = extract_tar(&target);
    let file_paths: Vec<&str> = entries
        .keys()
        .filter(|k| !entries[*k].is_empty() || !k.ends_with('/'))
        .map(String::as_str)
        .collect();
    assert!(file_paths.contains(&"root.txt"));
    assert!(file_paths.contains(&"sub/inner.txt"));
    assert_eq!(entries["root.txt"], b"root");
    assert_eq!(entries["sub/inner.txt"], b"inner");
}
