//! Layer unpacking and stacking.
//!
//! See `docs/prd/layers.md` for the contract.

pub mod apply;
pub mod error;
pub mod stack;
pub mod whiteout;

pub use apply::{ApplyStats, apply};
pub use error::LayerError;
pub use stack::{StackStats, flatten, stack};
