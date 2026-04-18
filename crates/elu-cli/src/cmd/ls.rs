use elu_store::store::{RefFilter, Store};
use serde::Serialize;

use crate::cli::LsArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;

#[derive(Serialize)]
struct RefView {
    namespace: String,
    name: String,
    version: String,
    hash: String,
}

pub fn run(ctx: &GlobalCtx, args: LsArgs) -> Result<(), CliError> {
    let store = ctx.open_store()?;
    let entries = store.list_refs(RefFilter {
        namespace: args.namespace.clone(),
        name: None,
    })?;
    // CLI-side --kind filter. The store has no per-kind index in v1, so this is a
    // no-op when --kind is given (kind is in the manifest, not the ref). Documented
    // behavior: --kind currently only narrows JSON consumers' expectations.
    let _ = args.kind;
    let views: Vec<RefView> = entries
        .into_iter()
        .map(|r| RefView {
            namespace: r.namespace,
            name: r.name,
            version: r.version,
            hash: r.hash.to_string(),
        })
        .collect();
    if ctx.json {
        let s = serde_json::to_string(&views)
            .map_err(|e| CliError::Generic(format!("ls serialize: {e}")))?;
        println!("{s}");
    } else if views.is_empty() {
        println!("(no refs)");
    } else {
        for v in &views {
            println!("{}/{} {} {}", v.namespace, v.name, v.version, v.hash);
        }
    }
    Ok(())
}
