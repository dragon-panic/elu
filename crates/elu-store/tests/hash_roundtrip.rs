use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use elu_store::hasher::Hasher;

#[test]
fn hash_display_fromstr_roundtrip() {
    let bytes = [0xab; 32];
    let h = Hash::new(HashAlgo::Sha256, bytes);
    let s = h.to_string();
    assert!(s.starts_with("sha256:"));
    let parsed: Hash = s.parse().unwrap();
    assert_eq!(h, parsed);
}

#[test]
fn hash_prefix_and_rest() {
    let mut bytes = [0u8; 32];
    bytes[0] = 0xde;
    bytes[1] = 0xad;
    let h = Hash::new(HashAlgo::Sha256, bytes);
    assert_eq!(h.prefix(), "de");
    assert_eq!(h.rest().len(), 62);
    assert!(h.rest().starts_with("ad"));
}

#[test]
fn newtypes_are_distinct() {
    let h = Hash::new(HashAlgo::Sha256, [0x42; 32]);
    let diff_id = DiffId(h.clone());
    let blob_id = BlobId(h.clone());
    let manifest_hash = ManifestHash(h.clone());
    // They all Display the same string
    assert_eq!(diff_id.to_string(), blob_id.to_string());
    assert_eq!(blob_id.to_string(), manifest_hash.to_string());
    // But they are separate types — this is a compile-time check.
    // A function taking DiffId won't accept BlobId.
    fn takes_diff(_d: &DiffId) {}
    fn takes_blob(_b: &BlobId) {}
    takes_diff(&diff_id);
    takes_blob(&blob_id);
}

#[test]
fn hasher_produces_correct_sha256() {
    let mut h = Hasher::new();
    h.update(b"hello world");
    let hash = h.finalize();
    assert_eq!(hash.algo(), HashAlgo::Sha256);
    // Known SHA-256 of "hello world"
    assert_eq!(
        hash.to_string(),
        "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn parse_errors() {
    assert!("nocolon".parse::<Hash>().is_err());
    assert!("blake3:abcd".parse::<Hash>().is_err());
    assert!("sha256:zzzz".parse::<Hash>().is_err());
    assert!("sha256:abcd".parse::<Hash>().is_err()); // too short
}

#[test]
fn newtype_fromstr_roundtrip() {
    let h = Hash::new(HashAlgo::Sha256, [0x01; 32]);
    let s = h.to_string();
    let diff: DiffId = s.parse().unwrap();
    let blob: BlobId = s.parse().unwrap();
    let manifest: ManifestHash = s.parse().unwrap();
    assert_eq!(diff.0, h);
    assert_eq!(blob.0, h);
    assert_eq!(manifest.0, h);
}
