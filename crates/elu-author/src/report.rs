use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable error codes for agent dispatch. Human prose lives in `message` / `hint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    SchemaUnsupported,
    PackageNamespaceInvalid,
    PackageNameInvalid,
    PackageKindInvalid,
    PackageDescriptionInvalid,
    PackageVersionInvalid,
    LayerMissingInclude,
    LayerMixedForm,
    LayerIncludeNoMatches,
    LayerAbsolutePath,
    LayerParentEscape,
    GlobInvalid,
    DepRefInvalid,
    DepVersionInvalid,
    HookOpUnknownType,
    HookOpBadPath,
    HookOpPathNotProduced,
    SensitivePattern,
    FileNotReadable,
    StoreError,
    TomlParse,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SchemaUnsupported => "schema-unsupported",
            Self::PackageNamespaceInvalid => "package-namespace-invalid",
            Self::PackageNameInvalid => "package-name-invalid",
            Self::PackageKindInvalid => "package-kind-invalid",
            Self::PackageDescriptionInvalid => "package-description-invalid",
            Self::PackageVersionInvalid => "package-version-invalid",
            Self::LayerMissingInclude => "layer-missing-include",
            Self::LayerMixedForm => "layer-mixed-form",
            Self::LayerIncludeNoMatches => "layer-include-no-matches",
            Self::LayerAbsolutePath => "layer-absolute-path",
            Self::LayerParentEscape => "layer-parent-escape",
            Self::GlobInvalid => "glob-invalid",
            Self::DepRefInvalid => "dep-ref-invalid",
            Self::DepVersionInvalid => "dep-version-invalid",
            Self::HookOpUnknownType => "hook-op-unknown-type",
            Self::HookOpBadPath => "hook-op-bad-path",
            Self::HookOpPathNotProduced => "hook-op-path-not-produced",
            Self::SensitivePattern => "sensitive-pattern",
            Self::FileNotReadable => "file-not-readable",
            Self::StoreError => "store-error",
            Self::TomlParse => "toml-parse",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub field: String,
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl Diagnostic {
    pub fn new(field: impl Into<String>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            code,
            message: message.into(),
            hint: String::new(),
            file: None,
            line: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<Diagnostic>,
    #[serde(default)]
    pub warnings: Vec<Diagnostic>,
}

impl Report {
    pub fn success() -> Self {
        Self {
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn error(d: Diagnostic) -> Self {
        Self {
            ok: false,
            errors: vec![d],
            warnings: Vec::new(),
        }
    }

    pub fn push_error(&mut self, d: Diagnostic) {
        self.ok = false;
        self.errors.push(d);
    }

    pub fn push_warning(&mut self, d: Diagnostic) {
        self.warnings.push(d);
    }

    /// Promote all warnings to errors.
    pub fn promote_warnings(&mut self) {
        if !self.warnings.is_empty() {
            self.ok = false;
            self.errors.append(&mut self.warnings);
        }
    }

    pub fn extend(&mut self, other: Report) {
        if !other.ok {
            self.ok = false;
        }
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

/// Map a `ManifestError` to a structured `Diagnostic`. Used by check / build.
pub fn from_manifest_err(err: &elu_manifest::ManifestError) -> Diagnostic {
    use elu_manifest::ManifestError as M;
    match err {
        M::TomlParse(e) => Diagnostic::new("", ErrorCode::TomlParse, e.to_string())
            .with_hint("fix TOML syntax"),
        M::TomlSerialize(e) => Diagnostic::new("", ErrorCode::TomlParse, e.to_string()),
        M::JsonParse(e) => Diagnostic::new("", ErrorCode::TomlParse, e.to_string()),
        M::UnsupportedSchema(v) => Diagnostic::new(
            "schema",
            ErrorCode::SchemaUnsupported,
            format!("schema version {v} not supported"),
        )
        .with_hint("set `schema = 1`"),
        M::InvalidNamespace(s) => Diagnostic::new(
            "package.namespace",
            ErrorCode::PackageNamespaceInvalid,
            format!("invalid namespace: {s}"),
        )
        .with_hint("use lowercase letters, digits, and hyphens"),
        M::InvalidName(s) => Diagnostic::new(
            "package.name",
            ErrorCode::PackageNameInvalid,
            format!("invalid name: {s}"),
        )
        .with_hint("use lowercase letters, digits, and hyphens"),
        M::InvalidPackageRef(s) => Diagnostic::new(
            "dependency[].ref",
            ErrorCode::DepRefInvalid,
            format!("invalid package ref: {s}"),
        )
        .with_hint("use `namespace/name` with lowercase identifiers"),
        M::InvalidKind(s) => Diagnostic::new(
            "package.kind",
            ErrorCode::PackageKindInvalid,
            format!("invalid kind: {s}"),
        ),
        M::InvalidDescription(_) => Diagnostic::new(
            "package.description",
            ErrorCode::PackageDescriptionInvalid,
            "description must be a non-empty single line",
        ),
        M::MixedLayerForm { index } => Diagnostic::new(
            format!("layer[{index}]"),
            ErrorCode::LayerMixedForm,
            format!("layer {index} mixes source and stored form"),
        )
        .with_hint("use either `include` or `diff_id`, not both"),
        M::LayerMissingField { index, field } => Diagnostic::new(
            format!("layer[{index}].{field}"),
            ErrorCode::LayerMissingInclude,
            format!("layer {index} missing required field: {field}"),
        ),
        M::HookOp { index, msg } => Diagnostic::new(
            format!("hook.op[{index}]"),
            ErrorCode::HookOpBadPath,
            msg.clone(),
        ),
        M::InvalidGlob(s) => Diagnostic::new("", ErrorCode::GlobInvalid, format!("invalid glob: {s}")),
        M::UnsafeLayerPath { index, field, msg } => {
            let code = if msg.contains("absolute") {
                ErrorCode::LayerAbsolutePath
            } else {
                ErrorCode::LayerParentEscape
            };
            Diagnostic::new(format!("layer[{index}].{field}"), code, msg.clone())
        }
    }
}
