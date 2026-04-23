mod common;

use camino::Utf8Path;
use elu_outputs::{DirOpts, Outcome, dir};
use tempfile::TempDir;

use common::{populate_staging, read};

#[test]
fn rename_staging_into_fresh_target() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join("out");

    let Outcome { bytes } = dir::materialize(&staging, &target, &DirOpts::default()).unwrap();

    assert!(target.as_std_path().is_dir());
    assert!(!staging.as_std_path().exists());
    assert_eq!(read(&target.join("root.txt")), "root");
    assert_eq!(read(&target.join("sub/inner.txt")), "inner");
    // 4 ("root") + 5 ("inner") = 9
    assert_eq!(bytes, 9);
}

#[test]
fn existing_target_refused_without_force() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join("out");
    std::fs::create_dir_all(target.as_std_path()).unwrap();
    std::fs::write(target.join("keep.txt").as_std_path(), b"keep").unwrap();

    let err = dir::materialize(&staging, &target, &DirOpts::default()).unwrap_err();
    assert!(matches!(
        err,
        elu_outputs::OutputError::TargetExists(_)
    ));
    assert_eq!(read(&target.join("keep.txt")), "keep");
}
