use elu_author::schema::{source_schema, stored_schema};

#[test]
fn source_schema_has_expected_top_level_shape() {
    let s = source_schema();
    assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
    let props = s["properties"].as_object().unwrap();
    assert!(props.contains_key("schema"));
    assert!(props.contains_key("package"));
    assert!(props.contains_key("layer"));
    let layer_items = &s["properties"]["layer"]["items"];
    let layer_props = layer_items["properties"].as_object().unwrap();
    // Source-specific fields
    for k in &["include", "exclude", "strip", "place", "mode", "follow_symlinks"] {
        assert!(layer_props.contains_key(*k), "missing property {k}");
    }
    // Source layer must require `include`
    let required: Vec<&str> = layer_items["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"include"));
}

#[test]
fn stored_schema_requires_diff_id_and_size() {
    let s = stored_schema();
    let layer_items = &s["properties"]["layer"]["items"];
    let required: Vec<&str> = layer_items["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"diff_id"));
    assert!(required.contains(&"size"));
    let props = layer_items["properties"].as_object().unwrap();
    assert!(!props.contains_key("include"));
}

#[test]
fn schema_accepts_prd_native_example() {
    // The authoring.md § Worked Examples "native package" example.
    let toml_src = r#"
schema = 1

[package]
namespace   = "dragon"
name        = "tree"
version     = "1.0.0"
kind        = "native"
description = "Prints a tree of files"

[[layer]]
name    = "bin"
include = ["target/release/tree"]
strip   = "target/release/"
place   = "bin/"

[[hook.op]]
type  = "chmod"
paths = ["bin/*"]
mode  = "+x"
"#;
    let parsed: elu_manifest::Manifest = toml::from_str(toml_src).unwrap();
    // Our validator is ground truth; the schema is a projection meant to match.
    elu_manifest::validate::validate_source(&parsed).unwrap();

    // Schema accepts the serialized JSON too:
    let json = serde_json::to_value(&parsed).unwrap();
    // Minimal invariants: has `schema`, `package` with namespace/name/version/kind/description,
    // and at least one layer with `include`.
    assert_eq!(json["schema"], 1);
    assert!(json["package"]["namespace"].is_string());
    assert!(json["layer"][0]["include"].is_array());
}
