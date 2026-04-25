//! Layer unpack primitives.
//!
//! Tar/zstd/gzip decoding, per-layer apply, whiteouts. Pure ring-2
//! crate: depends only on `elu-store`. Hooks + Resolution-aware
//! orchestration ("stack a Resolution into a directory and run hooks")
//! lives in `elu-stacker` above.
//!
//! See `docs/prd/layers.md` for the contract.

pub mod apply;
pub mod error;
pub mod whiteout;

pub use apply::{ApplyStats, apply};
pub use error::LayerError;
