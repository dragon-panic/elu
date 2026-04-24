use elu_registry::client::fallback::RegistryClient;
use elu_registry::client::publish::publish_package;

use crate::cli::PublishArgs;
use crate::error::CliError;
use crate::global::{DEFAULT_REGISTRY, GlobalCtx};
use crate::output::emit_event;
use crate::refs_parse::{parse_ref, Ref};

pub fn run(ctx: &GlobalCtx, args: PublishArgs) -> Result<(), CliError> {
    if ctx.offline {
        return Err(CliError::Network("--offline forbids registry contact".into()));
    }
    let token = args
        .token
        .ok_or_else(|| CliError::Usage("publish requires --token or ELU_PUBLISH_TOKEN".into()))?;

    let r = parse_ref(&args.reference)?;
    let (namespace, name, version) = match r {
        Ref::Exact { namespace, name, version } => (namespace, name, version),
        Ref::Hash(_) => {
            return Err(CliError::Usage(
                "publish requires <ns>/<name>@<version>; raw manifest hashes are not publishable".into(),
            ));
        }
    };

    let registry_str = ctx.registry.clone().unwrap_or_else(|| DEFAULT_REGISTRY.into());
    let client = RegistryClient::from_env_str(&registry_str)?;
    let store = ctx.open_store()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Generic(format!("tokio: {e}")))?;
    let record = rt.block_on(publish_package(
        &client, &store, &namespace, &name, &version, &token, None,
    ))?;

    if ctx.json {
        emit_event(
            ctx,
            &serde_json::json!({
                "event": "published",
                "namespace": record.namespace,
                "name": record.name,
                "version": record.version,
                "manifest_blob_id": record.manifest_blob_id.to_string(),
            }),
        );
    } else {
        println!("published {}/{}@{}", record.namespace, record.name, record.version);
    }
    Ok(())
}
