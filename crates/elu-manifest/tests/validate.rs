use elu_manifest::{from_toml_str, validate};

fn stored_manifest(overrides: &str) -> String {
    // Base valid stored manifest; overrides replace the [package] block or add sections
    format!(
        r#"
schema = 1

[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "A test package"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100

{overrides}
"#
    )
}

fn parse_and_validate_stored(toml: &str) -> Result<(), elu_manifest::ManifestError> {
    let m = from_toml_str(toml)?;
    validate::validate_stored(&m)
}

fn parse_and_validate_source(toml: &str) -> Result<(), elu_manifest::ManifestError> {
    let m = from_toml_str(toml)?;
    validate::validate_source(&m)
}

// --- Happy path ---

#[test]
fn valid_stored_manifest_passes() {
    let toml = stored_manifest("");
    parse_and_validate_stored(&toml).unwrap();
}

// --- Schema ---

#[test]
fn rejects_unsupported_schema() {
    let toml = r#"
schema = 99

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
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(
        err.to_string().contains("schema"),
        "error should mention schema: {err}"
    );
}

// --- Namespace ---

#[test]
fn rejects_uppercase_namespace() {
    let toml = r#"
schema = 1
[package]
namespace = "Test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(
        err.to_string().contains("namespace"),
        "error should mention namespace: {err}"
    );
}

#[test]
fn rejects_empty_namespace() {
    let toml = r#"
schema = 1
[package]
namespace = ""
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(err.to_string().contains("namespace"));
}

// --- Name ---

#[test]
fn rejects_name_with_underscore() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "my_pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(err.to_string().contains("name"));
}

// --- Kind ---

#[test]
fn rejects_empty_kind() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = ""
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(err.to_string().contains("kind"));
}

#[test]
fn rejects_kind_with_whitespace() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "ox skill"
description = "test"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(err.to_string().contains("kind"));
}

// --- Description ---

#[test]
fn rejects_empty_description() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = ""

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(err.to_string().contains("description"));
}

#[test]
fn rejects_multiline_description() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "line one\nline two"

[[layer]]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(err.to_string().contains("description"));
}

// --- Stored layer validation ---

#[test]
fn rejects_mixed_layer_form() {
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
include = ["src/**"]
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(
        err.to_string().contains("mix"),
        "error should mention mixing: {err}"
    );
}

#[test]
fn rejects_stored_layer_missing_size() {
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
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(
        err.to_string().contains("size"),
        "error should mention size: {err}"
    );
}

#[test]
fn rejects_source_layer_in_stored_validation() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
include = ["src/**"]
"#;
    let err = parse_and_validate_stored(toml).unwrap_err();
    assert!(
        err.to_string().contains("diff_id") || err.to_string().contains("stored"),
        "should reject source layer in stored validation: {err}"
    );
}

// --- Hook ops ---

#[test]
fn valid_hook_ops_pass() {
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
type = "chmod"
paths = ["bin/*"]
mode = "+x"
"#;
    parse_and_validate_stored(toml).unwrap();
}

// --- Source form validation ---

#[test]
fn valid_source_manifest_passes() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "A test package"

[[layer]]
include = ["src/**"]
name = "code"
"#;
    parse_and_validate_source(toml).unwrap();
}

#[test]
fn source_rejects_stored_layer() {
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
    let err = parse_and_validate_source(toml).unwrap_err();
    assert!(
        err.to_string().contains("include"),
        "should require include field: {err}"
    );
}

#[test]
fn source_rejects_mixed_layer() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
include = ["src/**"]
diff_id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
size = 100
"#;
    let err = parse_and_validate_source(toml).unwrap_err();
    assert!(
        err.to_string().contains("mix"),
        "should reject mixed layer: {err}"
    );
}

#[test]
fn source_rejects_invalid_glob() {
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = "native"
description = "test"

[[layer]]
include = ["[invalid"]
"#;
    let err = parse_and_validate_source(toml).unwrap_err();
    assert!(
        err.to_string().contains("glob"),
        "should mention glob: {err}"
    );
}

#[test]
fn source_validates_common_rules_too() {
    // Empty kind should fail even in source validation
    let toml = r#"
schema = 1
[package]
namespace = "test"
name = "pkg"
version = "1.0.0"
kind = ""
description = "test"

[[layer]]
include = ["src/**"]
"#;
    let err = parse_and_validate_source(toml).unwrap_err();
    assert!(err.to_string().contains("kind"));
}
