use std::process::ExitCode;

mod cli;
mod cmd;
mod error;
mod global;
mod lockfile;
mod manifest_reader;
mod output;
mod refs_parse;
mod source;

use crate::error::IntoExitCode;

fn main() -> ExitCode {
    let cli = match cli::parse() {
        Ok(c) => c,
        Err(e) => {
            // clap prints its own help/version/error message; map exit code.
            let code = match e.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayVersion => 0,
                _ => 2,
            };
            let _ = e.print();
            return ExitCode::from(code);
        }
    };
    cmd::dispatch(cli).into_exit_code()
}
