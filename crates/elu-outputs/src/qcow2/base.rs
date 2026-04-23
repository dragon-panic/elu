use elu_manifest::types::Manifest;

use crate::error::OutputError;

/// The `[metadata.os-base]` block of an `os-base` manifest, validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsBase {
    pub arch: String,
    pub kernel: String,
    pub init: String,
    pub finalize: Vec<String>,
}

/// Validate `manifest` as an `os-base` package and parse its
/// `[metadata.os-base]` block.
///
/// Returns [`OutputError::Base`] if `package.kind != "os-base"`, the block
/// is missing, or required fields are absent or the wrong type.
pub fn parse_os_base(manifest: &Manifest) -> Result<OsBase, OutputError> {
    if manifest.package.kind != "os-base" {
        return Err(OutputError::Base(format!(
            "base must have kind = \"os-base\", got \"{}\"",
            manifest.package.kind
        )));
    }
    let os_base = manifest
        .metadata
        .0
        .get("os-base")
        .ok_or_else(|| OutputError::Base("missing [metadata.os-base] block".to_string()))?;
    let table = os_base
        .as_table()
        .ok_or_else(|| OutputError::Base("[metadata.os-base] must be a table".to_string()))?;

    let arch = required_str(table, "arch")?;
    let kernel = required_str(table, "kernel")?;
    let init = required_str(table, "init")?;
    let finalize = match table.get("finalize") {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .ok_or_else(|| {
                OutputError::Base("[metadata.os-base].finalize must be an array".to_string())
            })?
            .iter()
            .map(|item| {
                item.as_str().map(str::to_owned).ok_or_else(|| {
                    OutputError::Base(
                        "[metadata.os-base].finalize entries must be strings".to_string(),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(OsBase {
        arch,
        kernel,
        init,
        finalize,
    })
}

fn required_str(table: &toml::value::Table, key: &str) -> Result<String, OutputError> {
    let v = table
        .get(key)
        .ok_or_else(|| OutputError::Base(format!("[metadata.os-base].{key} missing")))?;
    v.as_str()
        .map(str::to_owned)
        .ok_or_else(|| OutputError::Base(format!("[metadata.os-base].{key} must be a string")))
}
