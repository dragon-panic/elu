mod common;

use common::{Tar, env, store_plain, work};

use elu_layers::apply;

#[test]
fn second_apply_overwrites_file_content() {
    let e = env();
    let t1 = Tar::new()
        .file_mode_owned("a.txt", b"first", 0o644)
        .into_bytes();
    let t2 = Tar::new()
        .file_mode_owned("a.txt", b"second", 0o644)
        .into_bytes();

    apply(&e.store, &store_plain(&e, &t1), work(&e)).unwrap();
    apply(&e.store, &store_plain(&e, &t2), work(&e)).unwrap();

    assert_eq!(common::read_to_string(&work(&e).join("a.txt")), "second");
}

#[cfg(unix)]
#[test]
fn file_replaces_existing_symlink() {
    use std::os::unix::fs::symlink;

    let e = env();
    // Pre-place a symlink where the file will land.
    symlink("nowhere", work(&e).join("a.txt").as_std_path()).unwrap();

    let tar = Tar::new()
        .file_mode_owned("a.txt", b"content", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &tar), work(&e)).unwrap();

    let p = work(&e).join("a.txt");
    let meta = std::fs::symlink_metadata(p.as_std_path()).unwrap();
    assert!(meta.file_type().is_file(), "expected real file, got {meta:?}");
    assert_eq!(common::read_to_string(&p), "content");
}

#[cfg(unix)]
#[test]
fn symlink_replaces_existing_file_via_two_layers() {
    let e = env();
    // Layer 1: regular file at a.txt.
    let t1 = Tar::new()
        .file_mode_owned("a.txt", b"original", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t1), work(&e)).unwrap();

    // Layer 2: symlink at a.txt → elsewhere.
    let t2 = Tar::new().symlink("a.txt", "elsewhere").into_bytes();
    apply(&e.store, &store_plain(&e, &t2), work(&e)).unwrap();

    let target = std::fs::read_link(work(&e).join("a.txt").as_std_path()).unwrap();
    assert_eq!(target, std::path::Path::new("elsewhere"));
}
