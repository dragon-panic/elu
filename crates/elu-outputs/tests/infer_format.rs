use camino::Utf8Path;
use elu_outputs::{Compression, FormatName, infer_compression, infer_format};

#[test]
fn infer_format_by_extension() {
    assert_eq!(infer_format(Utf8Path::new("out.tar")), Some(FormatName::Tar));
    assert_eq!(
        infer_format(Utf8Path::new("out.tar.gz")),
        Some(FormatName::Tar)
    );
    assert_eq!(
        infer_format(Utf8Path::new("out.tgz")),
        Some(FormatName::Tar)
    );
    assert_eq!(
        infer_format(Utf8Path::new("out.tar.zst")),
        Some(FormatName::Tar)
    );
    assert_eq!(
        infer_format(Utf8Path::new("out.tar.xz")),
        Some(FormatName::Tar)
    );
    assert_eq!(
        infer_format(Utf8Path::new("disk.qcow2")),
        Some(FormatName::Qcow2)
    );
}

#[test]
fn no_extension_or_unknown_defaults_to_dir() {
    assert_eq!(infer_format(Utf8Path::new("./out")), Some(FormatName::Dir));
    assert_eq!(infer_format(Utf8Path::new("some/dir/")), Some(FormatName::Dir));
    assert_eq!(infer_format(Utf8Path::new("out.weird")), Some(FormatName::Dir));
}

#[test]
fn infer_compression_matches_suffix() {
    assert_eq!(infer_compression(Utf8Path::new("out.tar")), Compression::None);
    assert_eq!(infer_compression(Utf8Path::new("out.tar.gz")), Compression::Gzip);
    assert_eq!(infer_compression(Utf8Path::new("out.tgz")), Compression::Gzip);
    assert_eq!(
        infer_compression(Utf8Path::new("out.tar.zst")),
        Compression::Zstd
    );
    assert_eq!(
        infer_compression(Utf8Path::new("out.tar.xz")),
        Compression::Xz
    );
}
