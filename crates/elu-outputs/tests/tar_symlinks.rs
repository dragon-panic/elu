#![cfg(unix)]

mod common;

use std::fs;

use camino::Utf8Path;
use elu_outputs::{TarOpts, tar as tar_out};
use tempfile::TempDir;

#[test]
fn symlinks_round_trip_through_tar() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = work.join("staging");
    fs::create_dir_all(staging.as_std_path()).unwrap();
    fs::write(staging.join("real.txt").as_std_path(), b"real").unwrap();
    std::os::unix::fs::symlink("real.txt", staging.join("link").as_std_path()).unwrap();

    let target = work.join("out.tar");
    tar_out::materialize(&staging, &target, &TarOpts::default()).unwrap();

    let file = fs::File::open(target.as_std_path()).unwrap();
    let mut archive = tar::Archive::new(file);
    let mut found_symlink = false;
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        let header = entry.header();
        if header.entry_type() == tar::EntryType::Symlink {
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let link_name = header
                .link_name()
                .unwrap()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            assert_eq!(path, "link");
            assert_eq!(link_name, "real.txt");
            found_symlink = true;
        }
    }
    assert!(found_symlink, "no symlink entry in archive");
}
