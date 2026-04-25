use camino::Utf8PathBuf;

use elu_hooks::HookError;
use elu_layers::LayerError;

#[derive(thiserror::Error, Debug)]
pub enum StackError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("layer: {0}")]
    Layer(#[from] LayerError),

    #[error("hook: {0}")]
    Hook(#[from] HookError),

    #[error("target exists: {0}")]
    TargetExists(Utf8PathBuf),

    #[error("staging directory unavailable: {0}")]
    Staging(std::io::Error),
}
