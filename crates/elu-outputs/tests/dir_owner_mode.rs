#![cfg(unix)]

mod common;

use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;

use camino::Utf8Path;
use elu_outputs::{DirOpts, dir};
use tempfile::TempDir;

use common::populate_staging;

#[test]
fn mode_mask_clears_group_and_other_write() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    // Give the file wide perms so the mask has something to clear.
    std::fs::set_permissions(
        staging.join("root.txt").as_std_path(),
        std::fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    let target = work.join("out");

    let opts = DirOpts {
        mode_mask: Some(0o755),
        ..DirOpts::default()
    };
    dir::materialize(&staging, &target, &opts).unwrap();

    let mode = std::fs::metadata(target.join("root.txt").as_std_path())
        .unwrap()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o755);
}

#[test]
fn mode_mask_applies_recursively() {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    std::fs::set_permissions(
        staging.join("sub/inner.txt").as_std_path(),
        std::fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    let target = work.join("out");

    // 0o755 preserves dir traversal while stripping group/other write.
    let opts = DirOpts {
        mode_mask: Some(0o755),
        ..DirOpts::default()
    };
    dir::materialize(&staging, &target, &opts).unwrap();

    let mode = std::fs::metadata(target.join("sub/inner.txt").as_std_path())
        .unwrap()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o755);
}

#[test]
fn owner_non_current_user_returns_io_error_unprivileged() {
    // Without CAP_CHOWN, chown to a uid the process doesn't own returns
    // EPERM. Skip if the test is running as root (CI sometimes does).
    let euid = unsafe { libc_geteuid() };
    if euid == 0 {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join("out");

    // uid 65534 is conventionally nobody; using it as a "not us" value.
    let opts = DirOpts {
        owner: Some((65534, 65534)),
        ..DirOpts::default()
    };
    let err = dir::materialize(&staging, &target, &opts).unwrap_err();
    assert!(
        matches!(err, elu_outputs::OutputError::Io(_)),
        "expected io error, got {err:?}"
    );
}

#[test]
fn owner_to_current_user_succeeds() {
    // Setting to current uid/gid is always permitted and exercises the
    // owner code path without requiring root.
    let uid = unsafe { libc_geteuid() };
    let gid = unsafe { libc_getegid() };

    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join("out");

    let opts = DirOpts {
        owner: Some((uid, gid)),
        ..DirOpts::default()
    };
    dir::materialize(&staging, &target, &opts).unwrap();

    let m = std::fs::metadata(target.join("root.txt").as_std_path()).unwrap();
    assert_eq!(m.uid(), uid);
    assert_eq!(m.gid(), gid);
}

unsafe extern "C" {
    fn geteuid() -> u32;
    fn getegid() -> u32;
}

unsafe fn libc_geteuid() -> u32 {
    unsafe { geteuid() }
}

unsafe fn libc_getegid() -> u32 {
    unsafe { getegid() }
}
