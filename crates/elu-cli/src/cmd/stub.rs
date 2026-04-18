use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run<A>(verb: &str, task: &str, depends_on: &str, _ctx: &GlobalCtx, _args: A) -> Result<(), CliError> {
    Err(CliError::Generic(format!(
        "{verb} not yet implemented (depends on {task} {depends_on})"
    )))
}
