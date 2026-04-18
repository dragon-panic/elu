use elu_author::schema::{source_schema, stored_schema};

use crate::cli::SchemaArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(_ctx: &GlobalCtx, args: SchemaArgs) -> Result<(), CliError> {
    if args.yaml {
        return Err(CliError::Generic(
            "yaml schema output not implemented in v1; use json".into(),
        ));
    }
    let value = if args.stored {
        stored_schema()
    } else {
        source_schema()
    };
    let s = serde_json::to_string_pretty(&value)
        .map_err(|e| CliError::Generic(format!("schema serialize: {e}")))?;
    println!("{s}");
    Ok(())
}
