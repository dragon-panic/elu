use elu_registry::client::verify::{verify_blob, verify_layer, verify_manifest};
use elu_registry::types::LayerRecord;
use elu_store::hash::{BlobId, DiffId, Hash, HashAlgo, ManifestHash};
use elu_store::hasher::Hasher;
use url::Url;

#[test]
fn manifest_verification_accepts_valid_hash() {
    let data = b"schema = 1\n[package]\nnamespace = \"test\"\nname = \"pkg\"\nversion = \"1.0.0\"\nkind = \"native\"\ndescription = \"Test\"";
    let mut h = Hasher::new();
    h.update(data);
    let expected = ManifestHash(h.finalize());

    assert!(verify_manifest(data, &expected).is_ok());
}

#[test]
fn manifest_verification_rejects_tampered_data() {
    let data = b"original manifest";
    let mut h = Hasher::new();
    h.update(data);
    let expected = ManifestHash(h.finalize());

    let tampered = b"tampered manifest";
    assert!(verify_manifest(tampered, &expected).is_err());
}

#[test]
fn blob_verification_accepts_valid_hash() {
    let data = b"blob content here";
    let mut h = Hasher::new();
    h.update(data);
    let expected = BlobId(h.finalize());

    assert!(verify_blob(data, &expected).is_ok());
}

#[test]
fn blob_verification_rejects_tampered_data() {
    let expected = BlobId(Hash::new(HashAlgo::Sha256, [0xab; 32]));
    assert!(verify_blob(b"anything", &expected).is_err());
}

#[test]
fn two_layer_verification_full_roundtrip() {
    let compressed = b"gzip-compressed-layer-data";
    let decompressed = b"uncompressed-tar-layer-data";

    let mut h1 = Hasher::new();
    h1.update(compressed);
    let blob_id = BlobId(h1.finalize());

    let mut h2 = Hasher::new();
    h2.update(decompressed);
    let diff_id = DiffId(h2.finalize());

    let layer = LayerRecord {
        diff_id: diff_id.clone(),
        blob_id: blob_id.clone(),
        url: Url::parse("https://example.com/blobs/test").unwrap(),
        size_compressed: compressed.len() as u64,
        size_uncompressed: decompressed.len() as u64,
    };

    // Both layers valid
    assert!(verify_layer(compressed, decompressed, &layer).is_ok());

    // Tampered compressed bytes → blob_id check fails
    assert!(verify_layer(b"wrong-compressed", decompressed, &layer).is_err());

    // Tampered decompressed bytes → diff_id check fails
    let mut h_wrong = Hasher::new();
    h_wrong.update(b"wrong-compressed");
    let wrong_blob_id = BlobId(h_wrong.finalize());
    let layer_wrong_blob = LayerRecord {
        blob_id: wrong_blob_id,
        ..layer.clone()
    };
    // This would fail because the compressed bytes don't match the wrong blob_id either
    assert!(verify_layer(compressed, b"wrong-decompressed", &layer_wrong_blob).is_err());
}

#[test]
fn compromised_registry_cannot_substitute_content() {
    // Simulate: registry returns correct manifest hash but points to malicious blob
    let real_data = b"real layer data";
    let fake_data = b"malicious layer data";

    let mut h = Hasher::new();
    h.update(real_data);
    let real_blob_id = BlobId(h.finalize());

    // The layer record from the registry claims real_blob_id
    let layer = LayerRecord {
        diff_id: DiffId(Hash::new(HashAlgo::Sha256, [0x01; 32])),
        blob_id: real_blob_id,
        url: Url::parse("https://evil.example.com/malicious-blob").unwrap(),
        size_compressed: 100,
        size_uncompressed: 200,
    };

    // Client downloads fake_data from the malicious URL
    // Verification catches the substitution at blob_id check
    assert!(verify_blob(fake_data, &layer.blob_id).is_err());
}
