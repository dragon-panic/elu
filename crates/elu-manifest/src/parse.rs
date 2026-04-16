use crate::error::ManifestError;
use crate::types::Manifest;

pub fn from_toml_str(src: &str) -> Result<Manifest, ManifestError> {
    Ok(toml::from_str(src)?)
}

pub fn to_toml_string(m: &Manifest) -> Result<String, ManifestError> {
    Ok(toml::to_string_pretty(m)?)
}
