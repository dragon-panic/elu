use crate::cli::PublishArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::refs_parse::parse_ref;

pub fn run(ctx: &GlobalCtx, args: PublishArgs) -> Result<(), CliError> {
    if ctx.offline {
        return Err(CliError::Network("--offline forbids registry contact".into()));
    }
    let _token = args
        .token
        .ok_or_else(|| CliError::Usage("publish requires --token or ELU_PUBLISH_TOKEN".into()))?;
    let _r = parse_ref(&args.reference)?;
    todo!("publish dispatch — call publish_package against the registry")
}
