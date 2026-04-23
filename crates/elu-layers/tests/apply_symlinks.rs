#![cfg(unix)]

mod common;

use std::os::unix::fs::symlink;

use common::{Tar, env, store_plain, work};

use elu_layers::apply;

#[test]
fn creates_symlink_pointing_at_target() {
    let e = env();
    let tar = Tar::new()
        .symlink("link", "real/target")
        .into_bytes();
    let diff_id = store_plain(&e, &tar);

    apply(&e.store, &diff_id, work(&e)).unwrap();

    let link_path = work(&e).join("link");
    let target = std::fs::read_link(link_path.as_std_path()).unwrap();
    assert_eq!(target, std::path::Path::new("real/target"));
}

#[test]
fn replaces_existing_entry_with_symlink() {
    let e = env();
    // Pre-existing regular file at the symlink's path.
    std::fs::write(work(&e).join("link").as_std_path(), b"old").unwrap();
    let tar = Tar::new()
        .symlink("link", "elsewhere")
        .into_bytes();
    let diff_id = store_plain(&e, &tar);

    apply(&e.store, &diff_id, work(&e)).unwrap();

    let link_path = work(&e).join("link");
    let target = std::fs::read_link(link_path.as_std_path()).unwrap();
    assert_eq!(target, std::path::Path::new("elsewhere"));
}

#[test]
fn replaces_existing_symlink() {
    let e = env();
    // Pre-existing symlink at the path.
    symlink("old/target", work(&e).join("link").as_std_path()).unwrap();
    let tar = Tar::new()
        .symlink("link", "new/target")
        .into_bytes();
    let diff_id = store_plain(&e, &tar);

    apply(&e.store, &diff_id, work(&e)).unwrap();

    let link_path = work(&e).join("link");
    let target = std::fs::read_link(link_path.as_std_path()).unwrap();
    assert_eq!(target, std::path::Path::new("new/target"));
}
