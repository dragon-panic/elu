use elu_store::hash::{BlobId, DiffId, ManifestHash};
use elu_store::hasher::Hasher;

use crate::error::RegistryError;
use crate::types::LayerRecord;

/// Verify that fetched manifest bytes match the expected manifest hash.
pub fn verify_manifest(bytes: &[u8], expected: &ManifestHash) -> Result<(), RegistryError> {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    let actual = ManifestHash(hasher.finalize());
    if actual != *expected {
        return Err(RegistryError::InvalidManifest {
            reason: format!(
                "manifest hash mismatch: expected {expected}, got {actual}"
            ),
        });
    }
    Ok(())
}

/// Verify that fetched blob bytes match the expected blob_id.
/// Returns the diff_id (hash of uncompressed content) if decompressed_bytes is provided.
pub fn verify_blob(
    compressed_bytes: &[u8],
    expected_blob_id: &BlobId,
) -> Result<(), RegistryError> {
    let mut hasher = Hasher::new();
    hasher.update(compressed_bytes);
    let actual = BlobId(hasher.finalize());
    if actual != *expected_blob_id {
        return Err(RegistryError::InvalidManifest {
            reason: format!(
                "blob hash mismatch: expected {expected_blob_id}, got {actual}"
            ),
        });
    }
    Ok(())
}

/// Verify the two-layer integrity of a fetched layer:
/// 1. Hash compressed bytes → must equal blob_id
/// 2. Hash decompressed bytes → must equal diff_id
pub fn verify_layer(
    compressed_bytes: &[u8],
    decompressed_bytes: &[u8],
    layer: &LayerRecord,
) -> Result<(), RegistryError> {
    // Layer 1: verify blob_id (hash of compressed bytes)
    verify_blob(compressed_bytes, &layer.blob_id)?;

    // Layer 2: verify diff_id (hash of decompressed bytes)
    let mut hasher = Hasher::new();
    hasher.update(decompressed_bytes);
    let actual_diff = DiffId(hasher.finalize());
    if actual_diff != layer.diff_id {
        return Err(RegistryError::InvalidManifest {
            reason: format!(
                "diff hash mismatch: expected {}, got {actual_diff}",
                layer.diff_id
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use elu_store::hash::{Hash, HashAlgo};

    #[test]
    fn verify_manifest_accepts_correct_hash() {
        let data = b"hello manifest";
        let mut hasher = Hasher::new();
        hasher.update(data);
        let expected = ManifestHash(hasher.finalize());

        assert!(verify_manifest(data, &expected).is_ok());
    }

    #[test]
    fn verify_manifest_rejects_wrong_hash() {
        let data = b"hello manifest";
        let wrong = ManifestHash(Hash::new(HashAlgo::Sha256, [0xff; 32]));

        assert!(verify_manifest(data, &wrong).is_err());
    }

    #[test]
    fn verify_blob_accepts_correct_hash() {
        let data = b"compressed blob data";
        let mut hasher = Hasher::new();
        hasher.update(data);
        let expected = BlobId(hasher.finalize());

        assert!(verify_blob(data, &expected).is_ok());
    }

    #[test]
    fn verify_blob_rejects_wrong_hash() {
        let data = b"compressed blob data";
        let wrong = BlobId(Hash::new(HashAlgo::Sha256, [0xff; 32]));

        assert!(verify_blob(data, &wrong).is_err());
    }

    #[test]
    fn verify_layer_two_layer_check() {
        let compressed = b"compressed bytes";
        let decompressed = b"decompressed bytes";

        let mut h1 = Hasher::new();
        h1.update(compressed);
        let blob_id = BlobId(h1.finalize());

        let mut h2 = Hasher::new();
        h2.update(decompressed);
        let diff_id = DiffId(h2.finalize());

        let layer = LayerRecord {
            diff_id,
            blob_id,
            url: url::Url::parse("https://example.com/blob").unwrap(),
            size_compressed: compressed.len() as u64,
            size_uncompressed: decompressed.len() as u64,
        };

        assert!(verify_layer(compressed, decompressed, &layer).is_ok());
    }

    #[test]
    fn verify_layer_fails_on_wrong_blob_id() {
        let compressed = b"compressed bytes";
        let decompressed = b"decompressed bytes";

        let mut h2 = Hasher::new();
        h2.update(decompressed);
        let diff_id = DiffId(h2.finalize());

        let layer = LayerRecord {
            diff_id,
            blob_id: BlobId(Hash::new(HashAlgo::Sha256, [0xff; 32])),
            url: url::Url::parse("https://example.com/blob").unwrap(),
            size_compressed: compressed.len() as u64,
            size_uncompressed: decompressed.len() as u64,
        };

        assert!(verify_layer(compressed, decompressed, &layer).is_err());
    }

    #[test]
    fn verify_layer_fails_on_wrong_diff_id() {
        let compressed = b"compressed bytes";
        let decompressed = b"decompressed bytes";

        let mut h1 = Hasher::new();
        h1.update(compressed);
        let blob_id = BlobId(h1.finalize());

        let layer = LayerRecord {
            diff_id: DiffId(Hash::new(HashAlgo::Sha256, [0xff; 32])),
            blob_id,
            url: url::Url::parse("https://example.com/blob").unwrap(),
            size_compressed: compressed.len() as u64,
            size_uncompressed: decompressed.len() as u64,
        };

        assert!(verify_layer(compressed, decompressed, &layer).is_err());
    }
}
