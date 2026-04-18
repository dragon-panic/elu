use crate::cli::PublishArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(_ctx: &GlobalCtx, _args: PublishArgs) -> Result<(), CliError> {
    // The publish protocol (POST manifest → upload session → blob PUTs → commit)
    // exceeds the proposal's scoped registry-client extension cap. CLI surface
    // is wired; dispatch fills in once a publish client lands.
    Err(CliError::Generic(
        "publish not yet implemented (depends on registry publish-client; see WKIW.Oe2G)".into(),
    ))
}
