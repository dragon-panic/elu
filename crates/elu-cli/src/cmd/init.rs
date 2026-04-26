use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use elu_author::init::{
    infer_name_from_path, init_builtin, init_from_template, BuiltinKind, InitOpts, TemplateProvider,
};
use elu_author::report::{Diagnostic, ErrorCode};
use elu_registry::client::fallback::RegistryClient;
use elu_store::hash::DiffId;
use tokio::runtime::Runtime;
use url::Url;

use crate::cli::{BuiltinKind as CliKind, InitArgs};
use crate::error::CliError;
use crate::global::{GlobalCtx, DEFAULT_REGISTRY};

pub fn run(ctx: &GlobalCtx, args: InitArgs) -> Result<(), CliError> {
    if let Some(template) = &args.template {
        return run_template(ctx, &args, template);
    }

    if let Some(from) = &args.from {
        let name = infer_name_from_path(from)?;
        let opts = InitOpts {
            name,
            namespace: args.namespace,
        };
        init_builtin(&args.path, BuiltinKind::Native, &opts)?;
        return Ok(());
    }

    let kind = args
        .kind
        .ok_or_else(|| CliError::Usage("init: --kind, --from, or --template is required".into()))?;
    let name = args
        .name
        .ok_or_else(|| CliError::Usage("init: --name is required for builtin --kind".into()))?;

    let opts = InitOpts {
        name,
        namespace: args.namespace,
    };
    init_builtin(&args.path, map_kind(kind), &opts)?;
    Ok(())
}

fn map_kind(k: CliKind) -> BuiltinKind {
    match k {
        CliKind::Native => BuiltinKind::Native,
        CliKind::OxSkill => BuiltinKind::OxSkill,
        CliKind::OxPersona => BuiltinKind::OxPersona,
        CliKind::OxRuntime => BuiltinKind::OxRuntime,
    }
}

fn run_template(ctx: &GlobalCtx, args: &InitArgs, template: &str) -> Result<(), CliError> {
    if ctx.offline {
        return Err(CliError::Network(
            "--offline forbids registry contact for init --template".into(),
        ));
    }
    let (ns, name, version) = parse_template_ref(template)?;
    let registry_str = ctx.registry.clone().unwrap_or_else(|| DEFAULT_REGISTRY.into());
    let client = RegistryClient::from_env_str(&registry_str)?;
    let rt = Arc::new(
        Runtime::new().map_err(|e| CliError::Generic(format!("tokio: {e}")))?,
    );
    let provider = RegistryTemplateProvider {
        client,
        rt,
        layer_urls: RefCell::new(HashMap::new()),
    };
    init_from_template(&args.path, &ns, &name, version.as_deref(), &provider)?;
    Ok(())
}

fn parse_template_ref(s: &str) -> Result<(String, String, Option<String>), CliError> {
    let (lhs, version) = match s.rsplit_once('@') {
        Some((l, v)) if !v.is_empty() => (l, Some(v.to_string())),
        Some(_) => return Err(CliError::Usage(format!("template ref `{s}` has empty version"))),
        None => (s, None),
    };
    let (ns, name) = lhs
        .split_once('/')
        .ok_or_else(|| CliError::Usage(format!("template ref must be `<ns>/<name>[@<version>]`, got: {s}")))?;
    Ok((ns.to_string(), name.to_string(), version))
}

struct RegistryTemplateProvider {
    client: RegistryClient,
    rt: Arc<Runtime>,
    layer_urls: RefCell<HashMap<String, Url>>,
}

impl TemplateProvider for RegistryTemplateProvider {
    fn fetch_manifest(
        &self,
        namespace: &str,
        name: &str,
        version: Option<&str>,
    ) -> Result<Vec<u8>, Diagnostic> {
        let v = version.ok_or_else(|| {
            Diagnostic::new(
                "init --template",
                ErrorCode::StoreError,
                "version required (use ns/name@version)",
            )
        })?;
        let record = self
            .rt
            .block_on(self.client.fetch_package(namespace, name, v))
            .map_err(|e| {
                Diagnostic::new("init --template", ErrorCode::StoreError, e.to_string())
            })?;
        let mut urls = self.layer_urls.borrow_mut();
        for layer in &record.layers {
            urls.insert(layer.diff_id.to_string(), layer.url.clone());
        }
        drop(urls);
        let bytes = self
            .rt
            .block_on(self.client.fetch_bytes(&record.manifest_url))
            .map_err(|e| {
                Diagnostic::new("init --template", ErrorCode::StoreError, e.to_string())
            })?;
        Ok(bytes)
    }

    fn fetch_blob(&self, diff_id: &DiffId) -> Result<Vec<u8>, Diagnostic> {
        let url = self
            .layer_urls
            .borrow()
            .get(&diff_id.to_string())
            .cloned()
            .ok_or_else(|| {
                Diagnostic::new(
                    "init --template",
                    ErrorCode::StoreError,
                    format!("no URL cached for diff_id {diff_id} — fetch_manifest must run first"),
                )
            })?;
        let bytes = self.rt.block_on(self.client.fetch_bytes(&url)).map_err(|e| {
            Diagnostic::new("init --template", ErrorCode::StoreError, e.to_string())
        })?;
        Ok(bytes)
    }
}
