use elu_author::init::{init_builtin, BuiltinKind, InitOpts};

use crate::cli::{BuiltinKind as CliKind, InitArgs};
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(_ctx: &GlobalCtx, args: InitArgs) -> Result<(), CliError> {
    if args.from.is_some() {
        return Err(CliError::Generic(
            "init --from inference not implemented in v1".into(),
        ));
    }
    if args.template.is_some() {
        return Err(CliError::Generic(
            "init --template not implemented in v1 (depends on registry template fetcher)".into(),
        ));
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
