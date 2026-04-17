use camino::Utf8PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum HookError {
    #[error("op {index} failed: {source}")]
    Op { index: usize, source: Box<HookError> },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid mode: {0}")]
    InvalidMode(String),

    #[error("path escapes staging: {0}")]
    PathEscape(String),

    #[error("glob: {0}")]
    Glob(String),

    #[error("symlink already exists: {0}")]
    SymlinkExists(Utf8PathBuf),

    #[error("file already exists: {0}")]
    FileExists(Utf8PathBuf),

    #[error("patch failed: {0}")]
    PatchFailed(Utf8PathBuf),

    #[error("unknown interpolation: {0}")]
    UnknownInterpolation(String),

    #[error("unclosed interpolation brace")]
    UnclosedBrace,

    #[error("diffy: {0}")]
    Diffy(String),
}
