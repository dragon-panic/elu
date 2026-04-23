mod common;

use common::{Tar, env, store_plain, work};

use elu_layers::apply;

#[test]
fn creates_nested_directories() {
    let e = env();
    let tar = Tar::new()
        .dir("a", 0o755)
        .dir("a/b", 0o755)
        .file_mode_owned("a/b/c.txt", b"hi", 0o644)
        .into_bytes();
    let diff_id = store_plain(&e, &tar);

    apply(&e.store, &diff_id, work(&e)).unwrap();

    let dir = work(&e).join("a/b");
    assert!(dir.as_std_path().is_dir());
    let f = work(&e).join("a/b/c.txt");
    assert_eq!(common::read_to_string(&f), "hi");
}

#[cfg(unix)]
#[test]
fn directory_mode_applied() {
    use std::os::unix::fs::PermissionsExt;
    let e = env();
    let tar = Tar::new().dir("private", 0o700).into_bytes();
    let diff_id = store_plain(&e, &tar);

    apply(&e.store, &diff_id, work(&e)).unwrap();

    let p = work(&e).join("private");
    let mode = std::fs::metadata(p.as_std_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
}

#[cfg(unix)]
#[test]
fn directory_mode_updated_on_collision() {
    use std::os::unix::fs::PermissionsExt;
    let e = env();
    // First layer: dir with 0o755
    let tar1 = Tar::new().dir("d", 0o755).into_bytes();
    let did1 = store_plain(&e, &tar1);
    apply(&e.store, &did1, work(&e)).unwrap();

    // Second layer: same dir with 0o700
    let tar2 = Tar::new().dir("d", 0o700).into_bytes();
    let did2 = store_plain(&e, &tar2);
    apply(&e.store, &did2, work(&e)).unwrap();

    let mode = std::fs::metadata(work(&e).join("d").as_std_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
}
