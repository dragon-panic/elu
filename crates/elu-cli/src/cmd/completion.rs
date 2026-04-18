use std::io;

use clap::CommandFactory;

use crate::cli::{Cli, CompletionArgs};
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(_ctx: &GlobalCtx, args: CompletionArgs) -> Result<(), CliError> {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    clap_complete::generate(args.shell, &mut cmd, bin_name, &mut io::stdout());
    Ok(())
}
