pub mod canonical;
pub mod error;
pub mod parse;
pub mod types;
pub mod validate;

pub use canonical::to_canonical_json;
pub use error::ManifestError;
pub use parse::{from_toml_str, to_toml_string};
pub use types::*;

/// Compute the manifest's identity hash: SHA-256 over canonical JSON.
pub fn manifest_hash(m: &Manifest) -> elu_store::hash::ManifestHash {
    let json = to_canonical_json(m);
    let mut h = elu_store::hasher::Hasher::new();
    h.update(&json);
    elu_store::hash::ManifestHash(h.finalize())
}
