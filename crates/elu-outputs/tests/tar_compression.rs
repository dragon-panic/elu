mod common;

use std::collections::BTreeMap;
use std::io::Read;

use camino::Utf8Path;
use elu_outputs::{Compression, TarOpts, tar as tar_out};
use tempfile::TempDir;

use common::populate_staging;

fn read_all(mut r: impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    r.read_to_end(&mut buf).unwrap();
    buf
}

fn entries_of(tar_bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = tar::Archive::new(tar_bytes);
    let mut out = BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().into_owned();
        if entry.header().entry_type() == tar::EntryType::Regular {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).unwrap();
            out.insert(path, buf);
        }
    }
    out
}

fn assert_roundtrip(compress: Compression, ext: &str) {
    let tmp = TempDir::new().unwrap();
    let work = Utf8Path::from_path(tmp.path()).unwrap();
    let staging = populate_staging(work, "staging");
    let target = work.join(format!("out.tar.{ext}"));

    let opts = TarOpts {
        compress,
        ..TarOpts::default()
    };
    tar_out::materialize(&staging, &target, &opts).unwrap();

    let raw = std::fs::read(target.as_std_path()).unwrap();
    let tar_bytes = match compress {
        Compression::None => raw.clone(),
        Compression::Gzip => read_all(flate2::read::GzDecoder::new(raw.as_slice())),
        Compression::Zstd => read_all(zstd::stream::read::Decoder::new(raw.as_slice()).unwrap()),
        Compression::Xz => read_all(xz2::read::XzDecoder::new(raw.as_slice())),
    };

    let e = entries_of(&tar_bytes);
    assert_eq!(e["root.txt"], b"root");
    assert_eq!(e["sub/inner.txt"], b"inner");
}

#[test]
fn gzip_roundtrip() {
    assert_roundtrip(Compression::Gzip, "gz");
}

#[test]
fn zstd_roundtrip() {
    assert_roundtrip(Compression::Zstd, "zst");
}

#[test]
fn xz_roundtrip() {
    assert_roundtrip(Compression::Xz, "xz");
}
