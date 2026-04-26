#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    #[error("toml parse: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("toml serialize: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("json parse: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("schema version {0} not supported")]
    UnsupportedSchema(u32),

    #[error("invalid namespace: {0}")]
    InvalidNamespace(String),

    #[error("invalid name: {0}")]
    InvalidName(String),

    #[error("invalid package ref: {0}")]
    InvalidPackageRef(String),

    #[error("invalid kind: {0}")]
    InvalidKind(String),

    #[error("invalid description: {0}")]
    InvalidDescription(String),

    #[error("layer {index} mixes source and stored form")]
    MixedLayerForm { index: usize },

    #[error("layer {index} missing required field: {field}")]
    LayerMissingField { index: usize, field: &'static str },

    #[error("hook op {index}: {msg}")]
    HookOp { index: usize, msg: String },

    #[error("invalid glob pattern: {0}")]
    InvalidGlob(String),

    #[error("layer {index}.{field}: {msg}")]
    UnsafeLayerPath {
        index: usize,
        field: &'static str,
        msg: String,
    },
}
