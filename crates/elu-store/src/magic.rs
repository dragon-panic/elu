/// Detected compression encoding of blob bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobEncoding {
    Gzip,
    Zstd,
    PlainTar,
}

/// Sniff the encoding from the first bytes of a blob.
/// Needs at least 262 bytes to reliably detect plain tar.
pub fn sniff_encoding(buf: &[u8]) -> Option<BlobEncoding> {
    // Gzip: 1f 8b
    if buf.len() >= 2 && buf[0] == 0x1f && buf[1] == 0x8b {
        return Some(BlobEncoding::Gzip);
    }
    // Zstd: 28 b5 2f fd
    if buf.len() >= 4 && buf[0] == 0x28 && buf[1] == 0xb5 && buf[2] == 0x2f && buf[3] == 0xfd {
        return Some(BlobEncoding::Zstd);
    }
    // Plain tar: "ustar" at offset 257
    if buf.len() >= 262 && &buf[257..262] == b"ustar" {
        return Some(BlobEncoding::PlainTar);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_gzip() {
        let buf = [0x1f, 0x8b, 0x08, 0x00];
        assert_eq!(sniff_encoding(&buf), Some(BlobEncoding::Gzip));
    }

    #[test]
    fn detect_zstd() {
        let buf = [0x28, 0xb5, 0x2f, 0xfd, 0x00];
        assert_eq!(sniff_encoding(&buf), Some(BlobEncoding::Zstd));
    }

    #[test]
    fn detect_plain_tar() {
        let mut buf = vec![0u8; 512];
        buf[257..262].copy_from_slice(b"ustar");
        assert_eq!(sniff_encoding(&buf), Some(BlobEncoding::PlainTar));
    }

    #[test]
    fn unknown_returns_none() {
        let buf = [0x00, 0x00, 0x00, 0x00];
        assert_eq!(sniff_encoding(&buf), None);
    }
}
