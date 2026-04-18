use std::fs;
use std::io::Cursor;

use camino::Utf8Path;
use elu_author::tar_det::{build_deterministic_tar, TarEntry};

fn read_tar_paths(bytes: &[u8]) -> Vec<String> {
    let mut ar = tar::Archive::new(Cursor::new(bytes));
    ar.entries()
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn tar_entries_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::write(root.join("b.txt"), b"b").unwrap();
    fs::write(root.join("a.txt"), b"a").unwrap();
    fs::write(root.join("c.txt"), b"c").unwrap();

    let entries = vec![
        TarEntry::file(root.join("c.txt"), "c.txt".into(), None),
        TarEntry::file(root.join("a.txt"), "a.txt".into(), None),
        TarEntry::file(root.join("b.txt"), "b.txt".into(), None),
    ];

    let bytes = build_deterministic_tar(&entries).unwrap();
    let paths = read_tar_paths(&bytes);
    assert_eq!(paths, vec!["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn tar_zeros_uid_gid_and_mtime() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::write(root.join("x.txt"), b"x").unwrap();

    let entries = vec![TarEntry::file(root.join("x.txt"), "x.txt".into(), None)];
    let bytes = build_deterministic_tar(&entries).unwrap();

    let mut ar = tar::Archive::new(Cursor::new(&bytes));
    let mut it = ar.entries().unwrap();
    let e = it.next().unwrap().unwrap();
    let h = e.header();
    assert_eq!(h.uid().unwrap(), 0);
    assert_eq!(h.gid().unwrap(), 0);
    assert_eq!(h.mtime().unwrap(), 0);
}

#[test]
fn tar_same_tree_same_bytes_across_tempdirs() {
    fn build(tmp: &camino::Utf8Path) -> Vec<u8> {
        fs::write(tmp.join("hello"), b"hello").unwrap();
        fs::create_dir_all(tmp.join("sub")).unwrap();
        fs::write(tmp.join("sub/world"), b"world").unwrap();
        let entries = vec![
            TarEntry::file(tmp.join("hello"), "hello".into(), Some(0o644)),
            TarEntry::file(tmp.join("sub/world"), "sub/world".into(), Some(0o644)),
        ];
        build_deterministic_tar(&entries).unwrap()
    }

    let t1 = tempfile::tempdir().unwrap();
    let t2 = tempfile::tempdir().unwrap();
    let a = build(Utf8Path::from_path(t1.path()).unwrap());
    // Sleep so the fs mtimes would differ on a non-deterministic builder
    std::thread::sleep(std::time::Duration::from_millis(10));
    let b = build(Utf8Path::from_path(t2.path()).unwrap());

    assert_eq!(a, b, "tar bytes must be identical across temp dirs");
}

#[test]
fn tar_respects_explicit_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::write(root.join("bin"), b"X").unwrap();
    let entries = vec![TarEntry::file(root.join("bin"), "bin".into(), Some(0o755))];
    let bytes = build_deterministic_tar(&entries).unwrap();

    let mut ar = tar::Archive::new(Cursor::new(&bytes));
    let mut it = ar.entries().unwrap();
    let e = it.next().unwrap().unwrap();
    assert_eq!(e.header().mode().unwrap() & 0o777, 0o755);
}
