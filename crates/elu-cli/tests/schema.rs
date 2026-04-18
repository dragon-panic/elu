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
fn schema_yaml_unsupported_in_v1_exits_one() {
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args(["schema", "--yaml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
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
