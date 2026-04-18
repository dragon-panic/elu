use elu_author::check::{check, CheckOpts};

use crate::cli::CheckArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(ctx: &GlobalCtx, args: CheckArgs) -> Result<(), CliError> {
    if !args.path.join("elu.toml").as_std_path().exists() {
        return Err(CliError::Usage(format!(
            "no elu.toml in {}",
            args.path
        )));
    }
    let report = check(&args.path, &CheckOpts { strict: args.strict });
    if ctx.json {
        let s = serde_json::to_string(&report)
            .map_err(|e| CliError::Generic(format!("report serialize: {e}")))?;
        println!("{s}");
    } else {
        for d in &report.errors {
            eprintln!("error[{}]: {}: {}", d.code, d.field, d.message);
            if !d.hint.is_empty() {
                eprintln!("  hint: {}", d.hint);
            }
        }
        for d in &report.warnings {
            eprintln!("warning[{}]: {}: {}", d.code, d.field, d.message);
        }
    }
    if report.ok {
        Ok(())
    } else {
        Err(CliError::Usage("check failed".into()))
    }
}
