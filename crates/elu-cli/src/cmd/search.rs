use elu_registry::client::fallback::RegistryClient;
use elu_registry::types::SearchQuery;

use crate::cli::SearchArgs;
use crate::error::CliError;
use crate::global::{GlobalCtx, DEFAULT_REGISTRY};

pub fn run(ctx: &GlobalCtx, args: SearchArgs) -> Result<(), CliError> {
    if ctx.offline {
        return Err(CliError::Network("--offline forbids registry contact".into()));
    }
    let registry_str = ctx.registry.clone().unwrap_or_else(|| DEFAULT_REGISTRY.into());
    let client = RegistryClient::from_env_str(&registry_str)?;
    let query = SearchQuery {
        q: args.query,
        kind: args.kind,
        tag: args.tag,
        namespace: args.namespace,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio: {e}")))?;
    let response = rt.block_on(client.search(&query))?;
    if ctx.json {
        let s = serde_json::to_string(&response)
            .map_err(|e| CliError::Generic(format!("search serialize: {e}")))?;
        println!("{s}");
    } else if response.results.is_empty() {
        println!("(no results)");
    } else {
        for r in &response.results {
            println!(
                "{}/{}@{} — {}",
                r.namespace,
                r.name,
                r.version,
                r.description.as_deref().unwrap_or("")
            );
        }
    }
    Ok(())
}
