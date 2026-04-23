mod common;

use camino::Utf8Path;
use elu_outputs::{DirOpts, dir};
use tempfile::TempDir;

use common::{populate_staging, read};

#[test]
fn force_replaces_existing_directory_target() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join("out");
    std::fs::create_dir_all(target.as_std_path()).unwrap();
    std::fs::write(target.join("old.txt").as_std_path(), b"old").unwrap();

    let opts = DirOpts {
        force: true,
        ..DirOpts::default()
    };
    dir::materialize(&staging, &target, &opts).unwrap();

    assert!(!target.join("old.txt").as_std_path().exists());
    assert_eq!(read(&target.join("root.txt")), "root");
    assert_eq!(read(&target.join("sub/inner.txt")), "inner");
}

#[test]
fn force_replaces_existing_file_target() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join("out");
    std::fs::write(target.as_std_path(), b"i am a file").unwrap();

    let opts = DirOpts {
        force: true,
        ..DirOpts::default()
    };
    dir::materialize(&staging, &target, &opts).unwrap();

    assert!(target.as_std_path().is_dir());
    assert_eq!(read(&target.join("root.txt")), "root");
}
