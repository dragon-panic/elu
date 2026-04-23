#![cfg(unix)]

use std::fs;

use camino::Utf8Path;
use elu_outputs::qcow2::{self, OsBase};
use elu_outputs::{OutputError, Qcow2Opts};
use tempfile::TempDir;

fn stage_dir(path: &Utf8Path) {
    fs::create_dir_all(path.as_std_path()).unwrap();
    fs::write(path.join("a.txt").as_std_path(), b"a").unwrap();
}

fn fake_base_meta() -> OsBase {
    OsBase {
        arch: "amd64".into(),
        kernel: "linux-image-amd64".into(),
        init: "systemd".into(),
        finalize: vec![],
    }
}

#[test]
fn missing_external_binary_surfaces_external_error() {
    // This test is only meaningful when at least one of the required
    // binaries is absent; in the usual CI this is qemu-img.
    if qcow2::which("qemu-img").is_some() && qcow2::which("mke2fs").is_some() {
        return;
    }

    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let user = work.join("user");
    let base = work.join("base");
    stage_dir(&user);
    stage_dir(&base);

    let target = work.join("disk.qcow2");
    let err = qcow2::materialize(
        &user,
        &base,
        &fake_base_meta(),
        &target,
        &Qcow2Opts::default(),
    )
    .unwrap_err();
    assert!(matches!(err, OutputError::External(_)), "got {err:?}");
    assert!(!target.as_std_path().exists());
}
