use camino::Utf8PathBuf;

use crate::hash::{HashParseError, ManifestHash};

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("store root not found at {0}")]
    RootMissing(Utf8PathBuf),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unknown blob encoding")]
    UnknownEncoding,

    #[error("manifest is not valid UTF-8")]
    ManifestNotUtf8,

    #[error("ref conflict at {path}: existing={existing}, incoming={incoming}")]
    RefConflict {
        path: Utf8PathBuf,
        existing: String,
        incoming: ManifestHash,
    },

    #[error("invalid ref component: {0}")]
    InvalidRefComponent(String),

    #[error("hash parse error: {0}")]
    HashParse(#[from] HashParseError),

    #[error("rename failed to {to}: {err}")]
    Rename { to: Utf8PathBuf, err: std::io::Error },

    #[error("gc locked: another process is running gc")]
    GcBusy,

    #[error("lock i/o: {0}")]
    Lock(std::io::Error),

    #[error("manifest read error: {0}")]
    ManifestRead(String),
}
