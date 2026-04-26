use elu_store::hash::ManifestHash;
use elu_store::store::{RefFilter, Store};
use serde::Serialize;

use crate::cli::{RefsAction, RefsArgs};
use crate::error::CliError;
use crate::global::GlobalCtx;

#[derive(Serialize)]
struct RefView {
    namespace: String,
    name: String,
    version: String,
    hash: String,
}

pub fn run(ctx: &GlobalCtx, args: RefsArgs) -> Result<(), CliError> {
    let store = ctx.open_store()?;
    match args.action {
        RefsAction::Ls => {
            let entries = store.list_refs(RefFilter::default())?;
            if ctx.json {
                let views: Vec<RefView> = entries
                    .into_iter()
                    .map(|r| RefView {
                        namespace: r.namespace,
                        name: r.name,
                        version: r.version,
                        hash: r.hash.to_string(),
                    })
                    .collect();
                let s = serde_json::to_string(&views)
                    .map_err(|e| CliError::Generic(format!("refs serialize: {e}")))?;
                println!("{s}");
            } else {
                for r in entries {
                    println!("{}/{}/{} -> {}", r.namespace, r.name, r.version, r.hash);
                }
            }
            Ok(())
        }
        RefsAction::Set { spec, hash } => {
            let (ns, name, version) = parse_spec(&spec)?;
            let h: ManifestHash = hash
                .parse()
                .map_err(|e| CliError::Usage(format!("invalid hash: {e}")))?;
            store.put_ref(ns, name, version, &h)?;
            Ok(())
        }
        RefsAction::Rm { spec } => {
            let (ns, name, version) = parse_spec(&spec)?;
            store.remove_ref(ns, name, version)?;
            Ok(())
        }
    }
}

fn parse_spec(spec: &str) -> Result<(&str, &str, &str), CliError> {
    let parts: Vec<&str> = spec.split('/').collect();
    if parts.len() != 3 {
        return Err(CliError::Usage(format!(
            "ref spec must be `<ns>/<name>/<version>`, got: {spec}"
        )));
    }
    Ok((parts[0], parts[1], parts[2]))
}
