mod common;

use common::{Tar, env, store_plain, work};

use elu_layers::apply;

#[test]
fn applies_a_regular_file() {
    let e = env();
    let tar = Tar::new()
        .file_mode_owned("hello.txt", b"hi\n", 0o644)
        .into_bytes();
    let diff_id = store_plain(&e, &tar);

    let stats = apply(&e.store, &diff_id, work(&e)).unwrap();

    assert_eq!(stats.entries_applied, 1);
    let out = work(&e).join("hello.txt");
    assert!(out.as_std_path().exists(), "file not created: {out}");
    assert_eq!(common::read_to_string(&out), "hi\n");
}

#[cfg(unix)]
#[test]
fn applies_file_with_mode() {
    use std::os::unix::fs::PermissionsExt;
    let e = env();
    let tar = Tar::new()
        .file_mode_owned("script.sh", b"#!/bin/sh\n", 0o755)
        .into_bytes();
    let diff_id = store_plain(&e, &tar);

    apply(&e.store, &diff_id, work(&e)).unwrap();

    let p = work(&e).join("script.sh");
    let mode = std::fs::metadata(p.as_std_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o755);
}

#[test]
fn missing_diff_id_errors() {
    use elu_store::hash::{DiffId, Hash, HashAlgo};
    let e = env();
    let unknown = DiffId(Hash::new(HashAlgo::Sha256, [0xab; 32]));
    let err = apply(&e.store, &unknown, work(&e)).unwrap_err();
    assert!(matches!(err, elu_layers::LayerError::DiffNotFound(_)));
}
