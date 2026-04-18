use std::fs;

use toml::Value;

use crate::cli::{ConfigAction, ConfigArgs};
use crate::error::CliError;
use crate::global::{config_dir, GlobalCtx};

pub fn run(ctx: &GlobalCtx, args: ConfigArgs) -> Result<(), CliError> {
    let path = config_dir().join("config.toml");
    match args.action {
        ConfigAction::Show => {
            let body = fs::read_to_string(path.as_std_path()).unwrap_or_default();
            let table: toml::Table = toml::from_str(&body)
                .map_err(|e| CliError::Generic(format!("config parse: {e}")))?;
            if ctx.json {
                let s = serde_json::to_string(&table)
                    .map_err(|e| CliError::Generic(format!("config json: {e}")))?;
                println!("{s}");
            } else {
                let s = toml::to_string_pretty(&table)
                    .map_err(|e| CliError::Generic(format!("config render: {e}")))?;
                print!("{s}");
            }
            Ok(())
        }
        ConfigAction::Set { key, value } => {
            let body = fs::read_to_string(path.as_std_path()).unwrap_or_default();
            let mut table: toml::Table = toml::from_str(&body)
                .map_err(|e| CliError::Generic(format!("config parse: {e}")))?;
            table.insert(key, Value::String(value));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent.as_std_path()).map_err(|e| {
                    CliError::Generic(format!("config mkdir {parent}: {e}"))
                })?;
            }
            let body = toml::to_string_pretty(&table)
                .map_err(|e| CliError::Generic(format!("config render: {e}")))?;
            fs::write(path.as_std_path(), body)
                .map_err(|e| CliError::Generic(format!("config write {path}: {e}")))?;
            Ok(())
        }
    }
}
