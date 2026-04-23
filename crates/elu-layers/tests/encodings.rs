mod common;

use common::{Tar, env, store_gzip, store_plain, store_zstd, work};

use elu_layers::apply;

fn one_file_tar() -> Vec<u8> {
    Tar::new()
        .file_mode_owned("greeting.txt", b"hello\n", 0o644)
        .into_bytes()
}

#[test]
fn unpack_plain_tar() {
    let e = env();
    let tar = one_file_tar();
    let did = store_plain(&e, &tar);
    apply(&e.store, &did, work(&e)).unwrap();
    assert_eq!(common::read_to_string(&work(&e).join("greeting.txt")), "hello\n");
}

#[test]
fn unpack_gzip() {
    let e = env();
    let tar = one_file_tar();
    let did = store_gzip(&e, &tar);
    apply(&e.store, &did, work(&e)).unwrap();
    assert_eq!(common::read_to_string(&work(&e).join("greeting.txt")), "hello\n");
}

#[test]
fn unpack_zstd() {
    let e = env();
    let tar = one_file_tar();
    let did = store_zstd(&e, &tar);
    apply(&e.store, &did, work(&e)).unwrap();
    assert_eq!(common::read_to_string(&work(&e).join("greeting.txt")), "hello\n");
}

#[test]
fn diff_id_is_stable_across_encodings() {
    // Same logical tar, three encodings: same diff_id.
    let e = env();
    let tar = one_file_tar();
    let plain = store_plain(&e, &tar);
    let gz = store_gzip(&e, &tar);
    let zst = store_zstd(&e, &tar);
    assert_eq!(plain, gz);
    assert_eq!(plain, zst);
}
