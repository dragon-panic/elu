use elu_manifest::{from_toml_str, Manifest};
use elu_store::hash::ManifestHash;
use elu_store::store::Store;

use crate::error::CliError;

/// Parsed package reference. v1 supports two forms:
/// `<ns>/<name>@<version>` (resolves via store ref) and `sha256:<hex>`
/// (direct manifest hash). Range refs (`@^0.3`) require a resolver and are
/// reported as a resolution failure.
pub enum Ref {
    Hash(ManifestHash),
    Exact { namespace: String, name: String, version: String },
}

pub fn parse_ref(input: &str) -> Result<Ref, CliError> {
    if let Ok(h) = input.parse::<ManifestHash>() {
        return Ok(Ref::Hash(h));
    }
    let (lhs, version) = input
        .rsplit_once('@')
        .ok_or_else(|| CliError::Usage(format!("ref must be `<ns>/<name>@<version>` or hash, got: {input}")))?;
    let (namespace, name) = lhs
        .split_once('/')
        .ok_or_else(|| CliError::Usage(format!("ref left of @ must be `<ns>/<name>`, got: {lhs}")))?;
    if version.starts_with(['^', '~', '*', '>', '<', '=']) || version.contains('*') {
        return Err(CliError::Resolution(format!(
            "version range resolution requires resolver (WKIW.wX0h); got: @{version}"
        )));
    }
    Ok(Ref::Exact {
        namespace: namespace.into(),
        name: name.into(),
        version: version.into(),
    })
}

pub fn load_manifest(store: &dyn Store, r: &Ref) -> Result<(ManifestHash, Manifest), CliError> {
    let hash = match r {
        Ref::Hash(h) => h.clone(),
        Ref::Exact { namespace, name, version } => store
            .get_ref(namespace, name, version)?
            .ok_or_else(|| CliError::Resolution(format!("not found in store: {namespace}/{name}@{version}")))?,
    };
    let bytes = store
        .get_manifest(&hash)?
        .ok_or_else(|| CliError::Store(format!("manifest blob missing: {hash}")))?;
    let manifest = parse_manifest_bytes(&bytes)?;
    Ok((hash, manifest))
}

fn parse_manifest_bytes(bytes: &[u8]) -> Result<Manifest, CliError> {
    if let Ok(m) = serde_json::from_slice::<Manifest>(bytes) {
        return Ok(m);
    }
    let s = std::str::from_utf8(bytes)
        .map_err(|_| CliError::Store("manifest is not utf-8".into()))?;
    from_toml_str(s).map_err(CliError::from)
}
