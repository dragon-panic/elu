use elu_manifest::{from_toml_str, to_canonical_json, to_toml_string, validate};

/// Unknown fields are preserved through TOML round-trip and through canonical
/// JSON serialization. PRD docs/prd/manifest.md:24: "unknown fields preserved
/// but ignored by elu itself."
#[test]
fn unknown_fields_preserved_through_toml_and_canonical_json() {
    let toml = r#"
schema = 1
custom_top_level = "hello"

[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"
custom_package_field = "world"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
custom_layer_field = "extra"
"#;
    let m = from_toml_str(toml).expect("unknown fields must not break parse");

    // 1. TOML round-trip: re-serialize and re-parse — unknown fields survive.
    let re_toml = to_toml_string(&m).unwrap();
    assert!(
        re_toml.contains("custom_top_level"),
        "top-level unknown field dropped on re-emit:\n{re_toml}",
    );
    assert!(
        re_toml.contains("custom_package_field"),
        "package unknown field dropped on re-emit:\n{re_toml}",
    );
    assert!(
        re_toml.contains("custom_layer_field"),
        "layer unknown field dropped on re-emit:\n{re_toml}",
    );

    // 2. Canonical JSON also retains unknown fields — consumers reading the
    // stored form (the hash-source) must see them too.
    let json_bytes = to_canonical_json(&m);
    let json: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
    assert_eq!(json["custom_top_level"], "hello");
    assert_eq!(json["package"]["custom_package_field"], "world");
    assert_eq!(json["layer"][0]["custom_layer_field"], "extra");
}

/// Empty optional collections should be omitted from serialized TOML.
#[test]
fn empty_collections_omitted_from_toml() {
    let toml = r#"
schema = 1

[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let m = from_toml_str(toml).unwrap();
    let output = to_toml_string(&m).unwrap();

    // Empty tags should not appear
    assert!(
        !output.contains("tags"),
        "empty tags should be omitted from TOML output"
    );
    // No dependency section
    assert!(
        !output.contains("[dependency]") && !output.contains("[[dependency]]"),
        "empty dependencies should be omitted"
    );
    // No hook section
    assert!(
        !output.contains("[hook]"),
        "empty hook should be omitted"
    );
    // No metadata section
    assert!(
        !output.contains("[metadata]"),
        "empty metadata should be omitted"
    );
}

/// Hook ops with absolute paths are rejected.
#[test]
fn hook_op_rejects_absolute_path() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100

[[hook.op]]
type = "mkdir"
path = "/etc/evil"
"#;
    let m = from_toml_str(toml).unwrap();
    let err = validate::validate_stored(&m).unwrap_err();
    assert!(
        err.to_string().contains("staging-relative") || err.to_string().contains("absolute"),
        "should reject absolute path: {err}"
    );
}

/// Hook ops with '..' path escapes are rejected.
#[test]
fn hook_op_rejects_dotdot_escape() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100

[[hook.op]]
type = "write"
path = "../escape/file"
content = "pwned"
"#;
    let m = from_toml_str(toml).unwrap();
    let err = validate::validate_stored(&m).unwrap_err();
    assert!(
        err.to_string().contains(".."),
        "should reject '..' in path: {err}"
    );
}

/// PackageRef rejects invalid formats.
#[test]
fn invalid_package_ref_rejected_at_parse() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[dependency]]
ref = "no-slash"
"#;
    let err = from_toml_str(toml).unwrap_err();
    assert!(
        err.to_string().contains("package ref") || err.to_string().contains("invalid"),
        "should reject invalid package ref: {err}"
    );
}

#[test]
fn package_ref_rejects_uppercase() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[dependency]]
ref = "Test/Pkg"
"#;
    let err = from_toml_str(toml).unwrap_err();
    assert!(
        err.to_string().contains("package ref") || err.to_string().contains("invalid"),
        "should reject uppercase package ref: {err}"
    );
}

/// Namespace allows dashes and digits.
#[test]
fn valid_namespace_with_dashes_and_digits() {
    let toml = r#"
schema = 1
[package]
namespace = "ox-community2"
name = "my-pkg-3"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let m = from_toml_str(toml).unwrap();
    validate::validate_stored(&m).unwrap();
}

/// VersionSpec "*" round-trips through TOML.
#[test]
fn version_spec_any_roundtrips() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[dependency]]
ref = "other/dep"
version = "*"
"#;
    let m1 = from_toml_str(toml).unwrap();
    assert!(matches!(
        &m1.dependencies[0].version,
        elu_manifest::VersionSpec::Any
    ));
    let out = to_toml_string(&m1).unwrap();
    let m2 = from_toml_str(&out).unwrap();
    assert_eq!(m1, m2);
}

/// Patch hook op with inline diff.
#[test]
fn patch_hook_op_inline() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100

[[hook.op]]
type = "patch"
file = "config.toml"
diff = "--- a\n+++ b\n"
"#;
    let m = from_toml_str(toml).unwrap();
    validate::validate_stored(&m).unwrap();
    assert!(matches!(
        &m.hook.ops[0],
        elu_manifest::HookOp::Patch {
            source: elu_manifest::PatchSource::Inline { .. },
            ..
        }
    ));
}

/// Template hook op.
#[test]
fn template_hook_op() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100

[[hook.op]]
type = "template"
input = "etc/config.tmpl"
output = "etc/config"

[hook.op.vars]
version = "1.0.0"
"#;
    let m = from_toml_str(toml).unwrap();
    validate::validate_stored(&m).unwrap();
    assert!(matches!(&m.hook.ops[0], elu_manifest::HookOp::Template { vars, .. } if vars.contains_key("version")));
}

/// Move hook op.
#[test]
fn move_hook_op() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100

[[hook.op]]
type = "move"
from = "old/path"
to = "new/path"
"#;
    let m = from_toml_str(toml).unwrap();
    validate::validate_stored(&m).unwrap();
}
