//! `elu install` — fetch a package from the configured registry into the
//! local store, then materialize it into an output directory.
//!
//! Slice 3 of the registry round-trip arc (cx SnIt). v1 is intentionally
//! narrow: a single explicit `<ns>/<name>@<version>` ref is fetched from the
//! registry and stacked into `--out` (default `./elu-out`). Range refs and
//! transitive registry resolution land in later slices.

use crate::cli::InstallArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(_ctx: &GlobalCtx, _args: InstallArgs) -> Result<(), CliError> {
    todo!("install dispatch — fetch from registry, verify, populate store, stack to --out")
}
