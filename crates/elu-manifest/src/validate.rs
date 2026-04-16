use crate::error::ManifestError;
use crate::types::{HookOp, Layer, Manifest};

/// Validate a stored-form manifest (all layers must have diff_id + size).
pub fn validate_stored(m: &Manifest) -> Result<(), ManifestError> {
    validate_common(m)?;
    for (i, layer) in m.layers.iter().enumerate() {
        reject_mixed(i, layer)?;
        if !layer.is_stored_form() {
            return Err(ManifestError::LayerMissingField {
                index: i,
                field: "diff_id",
            });
        }
        if layer.size.is_none() {
            return Err(ManifestError::LayerMissingField {
                index: i,
                field: "size",
            });
        }
        // Source-form fields must be absent
        if !layer.include.is_empty()
            || !layer.exclude.is_empty()
            || layer.strip.is_some()
            || layer.place.is_some()
            || layer.mode.is_some()
        {
            return Err(ManifestError::MixedLayerForm { index: i });
        }
    }
    Ok(())
}

/// Validate a source-form manifest (all layers must have include, no diff_id/size).
pub fn validate_source(m: &Manifest) -> Result<(), ManifestError> {
    validate_common(m)?;
    for (i, layer) in m.layers.iter().enumerate() {
        reject_mixed(i, layer)?;
        if !layer.is_source_form() {
            return Err(ManifestError::LayerMissingField {
                index: i,
                field: "include",
            });
        }
        if layer.diff_id.is_some() || layer.size.is_some() {
            return Err(ManifestError::MixedLayerForm { index: i });
        }
        // Validate globs parse
        for pattern in &layer.include {
            globset::GlobBuilder::new(pattern)
                .build()
                .map_err(|e| ManifestError::InvalidGlob(e.to_string()))?;
        }
        for pattern in &layer.exclude {
            globset::GlobBuilder::new(pattern)
                .build()
                .map_err(|e| ManifestError::InvalidGlob(e.to_string()))?;
        }
    }
    Ok(())
}

fn validate_common(m: &Manifest) -> Result<(), ManifestError> {
    // 1. Schema version
    if m.schema != 1 {
        return Err(ManifestError::UnsupportedSchema(m.schema));
    }

    // 2. Namespace: ^[a-z0-9][a-z0-9-]*$
    if !is_valid_ident(&m.package.namespace) {
        return Err(ManifestError::InvalidNamespace(
            m.package.namespace.clone(),
        ));
    }

    // 3. Name: same pattern
    if !is_valid_ident(&m.package.name) {
        return Err(ManifestError::InvalidName(m.package.name.clone()));
    }

    // 4. Version — enforced by semver::Version deserialization, so already valid.

    // 5. Kind: non-empty, no whitespace
    if m.package.kind.is_empty() || m.package.kind.contains(char::is_whitespace) {
        return Err(ManifestError::InvalidKind(m.package.kind.clone()));
    }

    // 6. Description: non-empty, single line
    if m.package.description.is_empty() || m.package.description.contains('\n') {
        return Err(ManifestError::InvalidDescription(
            m.package.description.clone(),
        ));
    }

    // 7. Dependency refs are validated at parse time by PackageRef::from_str

    // 8. Hook ops well-formedness
    for (i, op) in m.hook.ops.iter().enumerate() {
        validate_hook_op(i, op)?;
    }

    Ok(())
}

/// Reject layers that have both source and stored fields.
fn reject_mixed(index: usize, layer: &Layer) -> Result<(), ManifestError> {
    if layer.is_source_form() && layer.is_stored_form() {
        return Err(ManifestError::MixedLayerForm { index });
    }
    Ok(())
}

fn is_valid_ident(s: &str) -> bool {
    !s.is_empty()
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn validate_hook_op(index: usize, op: &HookOp) -> Result<(), ManifestError> {
    let err = |msg: String| ManifestError::HookOp { index, msg };

    match op {
        HookOp::Chmod { paths, mode } => {
            if paths.is_empty() {
                return Err(err("chmod: paths must not be empty".into()));
            }
            if mode.is_empty() {
                return Err(err("chmod: mode must not be empty".into()));
            }
            reject_absolute_paths(index, paths)?;
        }
        HookOp::Mkdir { path, .. } => {
            reject_absolute_path(index, path)?;
        }
        HookOp::Symlink { from, to, .. } => {
            reject_absolute_path(index, from)?;
            reject_absolute_path(index, to)?;
        }
        HookOp::Write { path, content, .. } => {
            reject_absolute_path(index, path)?;
            if content.is_empty() {
                return Err(err("write: content must not be empty".into()));
            }
        }
        HookOp::Template {
            input, output, ..
        } => {
            reject_absolute_path(index, input)?;
            reject_absolute_path(index, output)?;
        }
        HookOp::Copy { from, to } => {
            reject_absolute_path(index, from)?;
            reject_absolute_path(index, to)?;
        }
        HookOp::Move { from, to } => {
            reject_absolute_path(index, from)?;
            reject_absolute_path(index, to)?;
        }
        HookOp::Delete { paths } => {
            if paths.is_empty() {
                return Err(err("delete: paths must not be empty".into()));
            }
            reject_absolute_paths(index, paths)?;
        }
        HookOp::Index { root, output, .. } => {
            reject_absolute_path(index, root)?;
            reject_absolute_path(index, output)?;
        }
        HookOp::Patch { file, .. } => {
            reject_absolute_path(index, file)?;
        }
    }
    Ok(())
}

fn reject_absolute_path(index: usize, path: &str) -> Result<(), ManifestError> {
    if path.starts_with('/') || path.contains("..") {
        return Err(ManifestError::HookOp {
            index,
            msg: format!("path must be staging-relative, no absolute or '..': {path}"),
        });
    }
    Ok(())
}

fn reject_absolute_paths(index: usize, paths: &[String]) -> Result<(), ManifestError> {
    for path in paths {
        reject_absolute_path(index, path)?;
    }
    Ok(())
}

// Note: Blob-existence checking is deliberately deferred to the resolver
// per the design doc (docs/design/manifest.md line 335). elu-manifest
// validates content/structure only; whether referenced layer blobs exist
// in the store is an elu-resolver concern.
