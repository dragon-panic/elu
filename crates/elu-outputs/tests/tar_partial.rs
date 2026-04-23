#![cfg(unix)]

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use camino::Utf8Path;
use elu_outputs::{TarOpts, tar as tar_out};
use tempfile::TempDir;

#[test]
fn write_failure_leaves_no_artifact_at_target() {
    // Running as root bypasses mode checks; skip in that case.
    let euid = unsafe { geteuid() };
    if euid == 0 {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = work.join("staging");
    fs::create_dir_all(staging.as_std_path()).unwrap();
    fs::write(staging.join("readable.txt").as_std_path(), b"ok").unwrap();
    fs::write(staging.join("unreadable.txt").as_std_path(), b"nope").unwrap();
    // chmod 0 — can't open for reading (unprivileged).
    fs::set_permissions(
        staging.join("unreadable.txt").as_std_path(),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();

    let target = work.join("out.tar");
    let err = tar_out::materialize(&staging, &target, &TarOpts::default()).unwrap_err();
    assert!(
        matches!(err, elu_outputs::OutputError::Io(_)),
        "expected io error, got {err:?}"
    );

    assert!(
        !target.as_std_path().exists(),
        "target should not exist after failure"
    );
    // .tmp sibling also cleaned up.
    let parent = target.parent().unwrap();
    let leftovers: Vec<_> = fs::read_dir(parent.as_std_path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(".out.tar.tmp")
        })
        .collect();
    assert!(leftovers.is_empty(), "tmp sibling leaked: {leftovers:?}");

    // Restore perms so TempDir can clean up.
    fs::set_permissions(
        staging.join("unreadable.txt").as_std_path(),
        fs::Permissions::from_mode(0o644),
    )
    .ok();
}

unsafe extern "C" {
    fn geteuid() -> u32;
}
