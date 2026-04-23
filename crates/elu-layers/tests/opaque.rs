mod common;

use common::{Tar, env, store_plain, work};

use elu_layers::apply;

#[test]
fn opaque_clears_dir_then_lays_layers_entries() {
    let e = env();
    // Layer 1: dir "etc" with two files.
    let t1 = Tar::new()
        .dir("etc", 0o755)
        .file_mode_owned("etc/old1", b"x", 0o644)
        .file_mode_owned("etc/old2", b"y", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t1), work(&e)).unwrap();

    // Layer 2: opaque whiteout in etc/, plus a new file. After applying:
    //   etc/old1 and etc/old2 must be gone, etc/new must be present.
    let t2 = Tar::new()
        .file_mode_owned("etc/.wh..wh..opq", b"", 0o644)
        .file_mode_owned("etc/new", b"fresh", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t2), work(&e)).unwrap();

    assert!(!work(&e).join("etc/old1").as_std_path().exists());
    assert!(!work(&e).join("etc/old2").as_std_path().exists());
    assert!(work(&e).join("etc/new").as_std_path().exists());
    // Marker itself never materialized.
    assert!(!work(&e).join("etc/.wh..wh..opq").as_std_path().exists());
}

#[test]
fn opaque_after_entries_in_tar_order_still_clears_then_keeps_them() {
    // Tar ordering may vary; PRD says opaque "removes every entry under
    // <parent>/ before applying this layer's entries in that directory".
    // Two-pass implementation must not delete this layer's own entries.
    let e = env();
    let t1 = Tar::new()
        .dir("d", 0o755)
        .file_mode_owned("d/old", b"x", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t1), work(&e)).unwrap();

    // Note: opaque entry comes AFTER the new entry in tar order.
    let t2 = Tar::new()
        .file_mode_owned("d/new", b"fresh", 0o644)
        .file_mode_owned("d/.wh..wh..opq", b"", 0o644)
        .into_bytes();
    apply(&e.store, &store_plain(&e, &t2), work(&e)).unwrap();

    assert!(!work(&e).join("d/old").as_std_path().exists());
    assert!(work(&e).join("d/new").as_std_path().exists());
}
