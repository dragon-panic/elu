use assert_cmd::Command;
use serde_json::Value;

#[test]
fn schema_emits_valid_json_with_source_layer_form_by_default() {
    let out = Command::cargo_bin("elu")
        .unwrap()
        .arg("schema")
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["title"], "elu.toml (source form)");
}

#[test]
fn schema_stored_emits_stored_form() {
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["schema", "--stored"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["title"], "elu.toml (stored form)");
}

#[test]
fn schema_source_emits_source_form() {
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["schema", "--source"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["title"], "elu.toml (source form)");
}

#[test]
fn schema_yaml_round_trips_to_default_json() {
    let yaml_out = Command::cargo_bin("elu")
        .unwrap()
        .args(["schema", "--yaml"])
        .output()
        .unwrap();
    assert!(
        yaml_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&yaml_out.stderr)
    );
    let json_out = Command::cargo_bin("elu").unwrap().arg("schema").output().unwrap();
    assert!(json_out.status.success());

    let from_yaml: Value = serde_norway::from_slice(&yaml_out.stdout)
        .expect("yaml output must parse as YAML");
    let from_json: Value = serde_json::from_slice(&json_out.stdout)
        .expect("json output must parse as JSON");
    assert_eq!(from_yaml, from_json, "yaml schema must round-trip to the json schema value");
}

#[test]
fn schema_help_lists_documented_flags() {
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["schema", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    for f in ["--stored", "--source", "--yaml"] {
        assert!(s.contains(f), "schema --help missing {f}: {s}");
    }
}
