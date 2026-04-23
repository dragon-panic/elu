use camino::Utf8PathBuf;

use elu_hooks::HookError;
use elu_store::error::StoreError;
use elu_store::hash::DiffId;

#[derive(thiserror::Error, Debug)]
pub enum LayerError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("store: {0}")]
    Store(#[from] StoreError),

    #[error("hook: {0}")]
    Hook(#[from] HookError),

    #[error("diff_id not in store: {0}")]
    DiffNotFound(DiffId),

    #[error("unknown blob encoding")]
    UnknownEncoding,

    #[error("unsafe tar entry path: {0}")]
    UnsafePath(String),

    #[error("non-utf8 tar entry path")]
    NonUtf8Path,

    #[error("target exists: {0}")]
    TargetExists(Utf8PathBuf),

    #[error("staging directory unavailable: {0}")]
    Staging(std::io::Error),
}
