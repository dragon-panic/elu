use elu_manifest::{from_toml_str, Manifest, VersionSpec};
use elu_store::error::StoreError;
use elu_store::hash::{DiffId, ManifestHash};
use elu_store::store::ManifestReader;

/// Production ManifestReader. The store keeps manifests in canonical JSON;
/// `from_toml_str` is the only public parser, so we deserialize via serde_json
/// directly into the same `Manifest` type.
pub struct ManifestParser;

impl ManifestReader for ManifestParser {
    fn layer_diff_ids(&self, bytes: &[u8]) -> Result<Vec<DiffId>, StoreError> {
        let m = parse(bytes)?;
        Ok(m.layers.into_iter().filter_map(|l| l.diff_id).collect())
    }

    fn dependency_hashes(&self, bytes: &[u8]) -> Result<Vec<ManifestHash>, StoreError> {
        let m = parse(bytes)?;
        Ok(m.dependencies
            .into_iter()
            .filter_map(|d| match d.version {
                VersionSpec::Pinned(h) => Some(h),
                _ => None,
            })
            .collect())
    }
}

fn parse(bytes: &[u8]) -> Result<Manifest, StoreError> {
    // Stored manifests are canonical JSON (see elu-manifest::canonical), so try
    // JSON first and fall back to TOML for the rare case where a caller wrote
    // raw TOML.
    if let Ok(m) = serde_json::from_slice::<Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes).map_err(|_| StoreError::ManifestNotUtf8)?;
    from_toml_str(s).map_err(|e| StoreError::ManifestRead(e.to_string()))
}
