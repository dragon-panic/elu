use elu_author::explain::{diff_manifests, diff_text, explain_text};

use crate::cli::ExplainArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::refs_parse::{load_manifest, parse_ref};

pub fn run(ctx: &GlobalCtx, args: ExplainArgs) -> Result<(), CliError> {
    if let Some(refs) = args.diff.as_deref() {
        return run_diff(ctx, refs);
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

fn run_diff(ctx: &GlobalCtx, refs: &[String]) -> Result<(), CliError> {
    let [old, new]: &[String; 2] = refs
        .try_into()
        .map_err(|_| CliError::Usage("explain --diff: expected exactly two refs".into()))?;
    let old_ref = parse_ref(old)?;
    let new_ref = parse_ref(new)?;
    let store = ctx.open_store()?;
    let (_, old_manifest) = load_manifest(&store, &old_ref)?;
    let (_, new_manifest) = load_manifest(&store, &new_ref)?;
    let diff = diff_manifests(&old_manifest, &new_manifest);
    if ctx.json {
        let s = serde_json::to_string(&diff)
            .map_err(|e| CliError::Generic(format!("explain --diff serialize: {e}")))?;
        println!("{s}");
    } else {
        print!("{}", diff_text(&diff));
    }
    Ok(())
}
