mod common;

use common::{Tar, env, store_plain, work};

use elu_layers::{LayerError, apply};

#[test]
fn rejects_parent_traversal() {
    let e = env();
    let tar = Tar::new()
        .raw_path("../escape", b"x")
        .into_bytes();
    let err = apply(&e.store, &store_plain(&e, &tar), work(&e)).unwrap_err();
    assert!(
        matches!(err, LayerError::UnsafePath(_)),
        "want UnsafePath, got {err:?}"
    );
    // Apply pre-creates the target dir, but the escaping entry was rejected
    // before any write — nothing inside target.
    let inside: Vec<_> = std::fs::read_dir(work(&e).as_std_path())
        .unwrap()
        .collect();
    assert!(inside.is_empty(), "target should be empty, got {inside:?}");
}

#[test]
fn rejects_absolute_paths() {
    let e = env();
    let tar = Tar::new()
        .raw_path("/etc/passwd", b"x")
        .into_bytes();
    let err = apply(&e.store, &store_plain(&e, &tar), work(&e)).unwrap_err();
    assert!(
        matches!(err, LayerError::UnsafePath(_)),
        "want UnsafePath, got {err:?}"
    );
}

#[test]
fn rejects_traversal_inside_path() {
    let e = env();
    let tar = Tar::new()
        .raw_path("d/../../escape", b"x")
        .into_bytes();
    let err = apply(&e.store, &store_plain(&e, &tar), work(&e)).unwrap_err();
    assert!(matches!(err, LayerError::UnsafePath(_)));
}
