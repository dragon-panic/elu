/// Verify that `type = "run"` in a manifest's hook ops is rejected
/// at parse time by elu-manifest's serde deserialization.
#[test]
fn run_op_rejected_at_parse_time() {
    let toml = r#"
schema = 1

[package]
namespace = "core"
name = "evil"
version = "0.1.0"
kind = "lib"
description = "tries to use run op"

[[hook.op]]
type = "run"
command = ["echo", "pwned"]
"#;
    let result = elu_manifest::from_toml_str(toml);
    assert!(result.is_err(), "type = 'run' should be rejected at parse time");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("run"),
        "error message should mention 'run', got: {err_msg}"
    );
}

#[test]
fn valid_ops_parse_fine() {
    let toml = r#"
schema = 1

[package]
namespace = "core"
name = "good"
version = "0.1.0"
kind = "lib"
description = "uses only declarative ops"

[[hook.op]]
type = "mkdir"
path = "bin"
parents = true

[[hook.op]]
type = "write"
path = "bin/hello.sh"
content = "echo hello"
"#;
    let manifest = elu_manifest::from_toml_str(toml).unwrap();
    assert_eq!(manifest.hook.ops.len(), 2);
}
