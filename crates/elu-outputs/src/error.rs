use camino::Utf8PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum OutputError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("target exists: {0}")]
    TargetExists(Utf8PathBuf),

    #[error("staging not a directory: {0}")]
    StagingNotDir(Utf8PathBuf),

    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    #[error("bad option: {0}")]
    BadOption(String),

    #[error("external tool: {0}")]
    External(String),

    #[error("base: {0}")]
    Base(String),
}
