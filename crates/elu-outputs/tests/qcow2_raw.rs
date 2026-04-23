//! Gated on `ELU_OUTPUTS_QCOW2=1`. Requires `mke2fs` on PATH.

#![cfg(unix)]

use std::fs;

use camino::Utf8Path;
use elu_outputs::qcow2;
use tempfile::TempDir;

fn gated() -> bool {
    std::env::var("ELU_OUTPUTS_QCOW2").as_deref() == Ok("1")
}

#[test]
fn mke2fs_builds_raw_ext4_from_directory() {
    if !gated() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let src = work.join("src");
    fs::create_dir_all(src.as_std_path()).unwrap();
    fs::write(src.join("hello.txt").as_std_path(), b"hi from image").unwrap();
    fs::create_dir_all(src.join("sub").as_std_path()).unwrap();
    fs::write(src.join("sub/inner.bin").as_std_path(), [7u8; 64]).unwrap();

    let raw = work.join("disk.raw");
    // 16 MiB — smallest viable ext4.
    qcow2::build_raw_ext4(&src, &raw, 16 * 1024 * 1024).unwrap();

    let meta = fs::metadata(raw.as_std_path()).unwrap();
    assert_eq!(meta.len(), 16 * 1024 * 1024);

    // ext4 superblock magic (0xEF53) lives at offset 1024 + 0x38.
    let mut f = fs::File::open(raw.as_std_path()).unwrap();
    use std::io::{Read, Seek, SeekFrom};
    f.seek(SeekFrom::Start(1024 + 0x38)).unwrap();
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic).unwrap();
    assert_eq!(
        u16::from_le_bytes(magic),
        0xEF53,
        "superblock magic not ext2/3/4",
    );
}
