use serde::Serialize;

use crate::error::CliError;
use crate::global::GlobalCtx;

/// Emit one streaming event line (NDJSON) under --json. No-op otherwise.
pub fn emit_event<T: Serialize>(ctx: &GlobalCtx, event: &T) {
    if ctx.json
        && let Ok(s) = serde_json::to_string(event)
    {
        println!("{s}");
    }
}

/// Print a CLI error. Always to stderr; under --json emits a structured event.
pub fn emit_error(ctx: &GlobalCtx, err: &CliError) {
    if ctx.json {
        let payload = serde_json::json!({
            "event": "error",
            "code": err.code_str(),
            "message": err.to_string(),
            "exit": err.exit_code(),
        });
        eprintln!("{}", payload);
    } else {
        eprintln!("error: {err}");
    }
}
