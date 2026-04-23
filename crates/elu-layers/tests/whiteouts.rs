mod common;

use common::{Tar, env, store_plain, work};

use elu_layers::apply;

#[test]
fn whiteout_deletes_existing_file() {
    let e = env();
    // Layer 1: foo present.
    let t1 = Tar::new()
        .file_mode_owned("foo", b"present", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t1), work(&e)).unwrap();
    assert!(work(&e).join("foo").as_std_path().exists());

    // Layer 2: .wh.foo deletes foo. The .wh. entry itself must not appear.
    let t2 = Tar::new()
        .file_mode_owned(".wh.foo", b"", 0o644)
        .into_bytes();
    let stats = apply(&e.store, &store_plain(&e, &t2), work(&e)).unwrap();

    assert!(!work(&e).join("foo").as_std_path().exists(), "foo not removed");
    assert!(
        !work(&e).join(".wh.foo").as_std_path().exists(),
        ".wh. entry must not be materialized"
    );
    assert_eq!(stats.whiteouts, 1);
}

#[test]
fn whiteout_in_subdir() {
    let e = env();
    let t1 = Tar::new()
        .dir("d", 0o755)
        .file_mode_owned("d/foo", b"x", 0o644)
        .file_mode_owned("d/bar", b"y", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t1), work(&e)).unwrap();

    let t2 = Tar::new()
        .file_mode_owned("d/.wh.foo", b"", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t2), work(&e)).unwrap();

    assert!(!work(&e).join("d/foo").as_std_path().exists());
    assert!(work(&e).join("d/bar").as_std_path().exists());
    assert!(!work(&e).join("d/.wh.foo").as_std_path().exists());
}

#[test]
fn whiteout_for_missing_path_is_noop() {
    let e = env();
    let tar = Tar::new()
        .file_mode_owned(".wh.never_existed", b"", 0o644)
        .into_bytes();
    let stats = apply(&e.store, &store_plain(&e, &tar), work(&e)).unwrap();
    assert_eq!(stats.whiteouts, 1);
}

#[test]
fn whiteout_removes_directory_subtree() {
    let e = env();
    let t1 = Tar::new()
        .dir("d", 0o755)
        .file_mode_owned("d/a", b"a", 0o644)
        .file_mode_owned("d/b", b"b", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t1), work(&e)).unwrap();

    // .wh.d at root deletes the whole directory subtree.
    let t2 = Tar::new()
        .file_mode_owned(".wh.d", b"", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t2), work(&e)).unwrap();

    assert!(!work(&e).join("d").as_std_path().exists());
}
