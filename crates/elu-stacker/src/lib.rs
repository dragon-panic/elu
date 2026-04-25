//! Resolve → unpack → run hooks orchestration.
//!
//! Sits above `elu-layers` (tar primitives), `elu-hooks` (declarative
//! op interpreter), and `elu-resolver` (dep graph + Resolution
//! type). Materializing a `Resolution` into a directory plus running
//! its post-unpack hook lives here, not in `elu-layers` — that's the
//! ring-model fix from cx WKIW.0CZW.
//!
//! See `docs/prd/layers.md` for the layer-stacking contract and
//! `docs/prd/hooks.md` for the hook surface.

pub mod error;
pub mod stage;

pub use error::StackError;
pub use stage::{Staging, StackStats, flatten, stack, stage};
