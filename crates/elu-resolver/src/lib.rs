//! Dependency resolver: turn references into a pinned, ordered, deduplicated
//! layer list ready for stacking.
//!
//! See `docs/prd/resolver.md` for the contract.

pub mod error;
pub mod lockfile;
pub mod resolve;
pub mod source;
pub mod types;
pub mod version;

pub use error::{Chain, ChainStep, ResolverError};
pub use lockfile::{Lockfile, LockfileEntry, lock, update, verify};
pub use resolve::resolve;
pub use source::{OfflineSource, VersionSource};
pub use types::{FetchItem, FetchPlan, ResolvedManifest, Resolution, RootRef};
