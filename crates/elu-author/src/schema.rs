use serde_json::{json, Value};

/// JSON Schema describing the source form of `elu.toml`.
pub fn source_schema() -> Value {
    let mut root = schema_root();
    root["title"] = json!("elu.toml (source form)");
    root["properties"]["layer"] = json!({
        "type": "array",
        "items": source_layer_schema(),
    });
    root
}

/// JSON Schema describing the stored form (what `elu build` emits).
pub fn stored_schema() -> Value {
    let mut root = schema_root();
    root["title"] = json!("elu.toml (stored form)");
    root["properties"]["layer"] = json!({
        "type": "array",
        "items": stored_layer_schema(),
    });
    root
}

fn schema_root() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["schema", "package"],
        "properties": {
            "schema": { "type": "integer", "const": 1 },
            "package": {
                "type": "object",
                "required": ["namespace", "name", "version", "kind", "description"],
                "properties": {
                    "namespace":   { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*$" },
                    "name":        { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*$" },
                    "version":     { "type": "string" },
                    "kind":        { "type": "string", "minLength": 1 },
                    "description": { "type": "string", "minLength": 1 },
                    "tags":        { "type": "array", "items": { "type": "string" } }
                },
                "additionalProperties": true
            },
            "dependency": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["ref"],
                    "properties": {
                        "ref":     { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*/[a-z0-9][a-z0-9-]*$" },
                        "version": { "type": "string" }
                    }
                }
            },
            "hook": {
                "type": "object",
                "properties": {
                    "op": {
                        "type": "array",
                        "items": hook_op_schema()
                    }
                }
            },
            "metadata": { "type": "object" }
        }
    })
}

fn source_layer_schema() -> Value {
    json!({
        "type": "object",
        "required": ["include"],
        "properties": {
            "name":            { "type": "string" },
            "include":         { "type": "array", "items": { "type": "string" }, "minItems": 1 },
            "exclude":         { "type": "array", "items": { "type": "string" } },
            "strip":           { "type": "string" },
            "place":           { "type": "string" },
            "mode":            { "type": "string" },
            "follow_symlinks": { "type": "boolean" }
        },
        "additionalProperties": false
    })
}

fn stored_layer_schema() -> Value {
    json!({
        "type": "object",
        "required": ["diff_id", "size"],
        "properties": {
            "name":    { "type": "string" },
            "diff_id": { "type": "string", "pattern": "^sha256:[a-f0-9]{64}$" },
            "size":    { "type": "integer", "minimum": 0 }
        },
        "additionalProperties": false
    })
}

fn hook_op_schema() -> Value {
    json!({
        "type": "object",
        "required": ["type"],
        "properties": {
            "type": {
                "type": "string",
                "enum": [
                    "chmod", "mkdir", "symlink", "write", "template",
                    "copy", "move", "delete", "index", "patch"
                ]
            }
        }
    })
}
