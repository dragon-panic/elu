use elu_author::explain::explain_text;

use crate::cli::InspectArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::refs_parse::{load_manifest, parse_ref};

pub fn run(ctx: &GlobalCtx, args: InspectArgs) -> Result<(), CliError> {
    let r = parse_ref(&args.reference)?;
    let store = ctx.open_store()?;
    let (_hash, manifest) = load_manifest(&store, &r)?;
    if ctx.json {
        let s = serde_json::to_string(&manifest)
            .map_err(|e| CliError::Generic(format!("inspect serialize: {e}")))?;
        println!("{s}");
    } else {
        print!("{}", explain_text(&manifest));
    }
    Ok(())
}
