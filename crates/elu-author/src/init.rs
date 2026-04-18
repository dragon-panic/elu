use std::fs;

use camino::Utf8Path;
use elu_store::hash::DiffId;

use crate::report::{Diagnostic, ErrorCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinKind {
    Native,
    OxSkill,
    OxPersona,
    OxRuntime,
}

impl BuiltinKind {
    fn template(&self) -> &'static str {
        match self {
            Self::Native => include_str!("templates/native.toml"),
            Self::OxSkill => include_str!("templates/ox_skill.toml"),
            Self::OxPersona => include_str!("templates/ox_persona.toml"),
            Self::OxRuntime => include_str!("templates/ox_runtime.toml"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InitOpts {
    pub name: String,
    pub namespace: String,
}

pub fn init_builtin(
    dir: &Utf8Path,
    kind: BuiltinKind,
    opts: &InitOpts,
) -> Result<(), Diagnostic> {
    let target = dir.join("elu.toml");
    if target.as_std_path().exists() {
        return Err(Diagnostic::new(
            "elu.toml",
            ErrorCode::StoreError,
            format!("{target} already exists"),
        )
        .with_hint("remove or rename the existing file before running init"));
    }

    let rendered = render(kind.template(), &opts.namespace, &opts.name);
    fs::write(target.as_std_path(), rendered).map_err(|e| {
        Diagnostic::new(
            "elu.toml",
            ErrorCode::StoreError,
            format!("write failed: {e}"),
        )
    })?;
    Ok(())
}

fn render(template: &str, namespace: &str, name: &str) -> String {
    template
        .replace("{{namespace}}", namespace)
        .replace("{{name}}", name)
}

/// Source of template packages. Satisfied by a registry client in
/// production and by a fake in tests.
pub trait TemplateProvider {
    fn fetch_manifest(
        &self,
        namespace: &str,
        name: &str,
        version: Option<&str>,
    ) -> Result<Vec<u8>, Diagnostic>;

    fn fetch_blob(&self, diff_id: &DiffId) -> Result<Vec<u8>, Diagnostic>;
}

pub fn init_from_template(
    dir: &Utf8Path,
    namespace: &str,
    name: &str,
    version: Option<&str>,
    provider: &dyn TemplateProvider,
) -> Result<(), Diagnostic> {
    let bytes = provider.fetch_manifest(namespace, name, version)?;
    let manifest: elu_manifest::Manifest = serde_json::from_slice(&bytes).map_err(|e| {
        Diagnostic::new(
            "",
            ErrorCode::StoreError,
            format!("template manifest parse: {e}"),
        )
    })?;

    for (i, layer) in manifest.layers.iter().enumerate() {
        let diff_id = layer.diff_id.as_ref().ok_or_else(|| {
            Diagnostic::new(
                format!("layer[{i}]"),
                ErrorCode::StoreError,
                "template layer missing diff_id",
            )
        })?;

        let blob = provider.fetch_blob(diff_id)?;

        let mut h = elu_store::hasher::Hasher::new();
        h.update(&blob);
        let computed = elu_store::hash::DiffId(h.finalize());
        if &computed != diff_id {
            return Err(Diagnostic::new(
                format!("layer[{i}]"),
                ErrorCode::StoreError,
                format!("diff_id mismatch for layer {i}: advertised {diff_id}, computed {computed}"),
            ));
        }

        extract_tar(dir, &blob)?;
    }

    Ok(())
}

fn extract_tar(dest: &Utf8Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
    for entry in archive.entries().map_err(|e| {
        Diagnostic::new("", ErrorCode::StoreError, format!("tar entries: {e}"))
    })? {
        let mut entry = entry.map_err(|e| {
            Diagnostic::new("", ErrorCode::StoreError, format!("tar entry: {e}"))
        })?;
        let path = entry
            .path()
            .map_err(|e| Diagnostic::new("", ErrorCode::StoreError, format!("tar path: {e}")))?
            .into_owned();
        let rel = path
            .to_str()
            .ok_or_else(|| Diagnostic::new("", ErrorCode::StoreError, "non-utf8 tar path"))?;
        // Reject absolute and parent-escape paths.
        if rel.starts_with('/') || rel.split('/').any(|s| s == "..") {
            return Err(Diagnostic::new(
                "",
                ErrorCode::StoreError,
                format!("unsafe path in template: {rel}"),
            ));
        }
        let out = dest.join(rel);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(out.as_std_path()).map_err(|e| {
                Diagnostic::new("", ErrorCode::StoreError, format!("mkdir {out}: {e}"))
            })?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent.as_std_path()).map_err(|e| {
                Diagnostic::new("", ErrorCode::StoreError, format!("mkdir {parent}: {e}"))
            })?;
        }
        let mut writer = fs::File::create(out.as_std_path()).map_err(|e| {
            Diagnostic::new("", ErrorCode::StoreError, format!("create {out}: {e}"))
        })?;
        std::io::copy(&mut entry, &mut writer).map_err(|e| {
            Diagnostic::new("", ErrorCode::StoreError, format!("write {out}: {e}"))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_placeholders() {
        let out = render("a={{namespace}} b={{name}}", "ns", "pkg");
        assert_eq!(out, "a=ns b=pkg");
    }
}
