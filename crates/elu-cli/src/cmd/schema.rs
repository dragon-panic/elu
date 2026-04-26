use elu_author::schema::{source_schema, stored_schema};

use crate::cli::SchemaArgs;
use crate::error::CliError;
use crate::global::GlobalCtx;

pub fn run(_ctx: &GlobalCtx, args: SchemaArgs) -> Result<(), CliError> {
    let value = if args.stored {
        stored_schema()
    } else {
        source_schema()
    };
    let s = if args.yaml {
        serde_norway::to_string(&value)
            .map_err(|e| CliError::Generic(format!("schema serialize: {e}")))?
    } else {
        serde_json::to_string_pretty(&value)
            .map_err(|e| CliError::Generic(format!("schema serialize: {e}")))?
    };
    println!("{s}");
    Ok(())
}
