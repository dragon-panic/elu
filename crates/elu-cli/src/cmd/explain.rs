use elu_author::explain::explain_text;

use crate::cli::ExplainArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::refs_parse::{load_manifest, parse_ref};

pub fn run(ctx: &GlobalCtx, args: ExplainArgs) -> Result<(), CliError> {
    if args.diff.is_some() {
        return Err(CliError::Generic(
            "explain --diff not implemented in v1".into(),
        ));
    }
    let reference = args
        .reference
        .ok_or_else(|| CliError::Usage("explain: <reference> required".into()))?;
    let r = parse_ref(&reference)?;
    let store = ctx.open_store()?;
    let (_hash, manifest) = load_manifest(&store, &r)?;
    if ctx.json {
        let s = serde_json::to_string(&manifest)
            .map_err(|e| CliError::Generic(format!("explain serialize: {e}")))?;
        println!("{s}");
    } else {
        print!("{}", explain_text(&manifest));
    }
    Ok(())
}
