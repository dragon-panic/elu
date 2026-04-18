use std::fs;

use camino::Utf8Path;
use elu_author::check::{check, CheckOpts};
use elu_author::report::ErrorCode;

fn write_manifest(root: &Utf8Path, body: &str) {
    fs::write(root.join("elu.toml"), body).unwrap();
}

#[test]
fn check_surfaces_unsupported_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    write_manifest(
        root,
        r#"
schema = 2
[package]
namespace = "x"
name = "y"
version = "0.1.0"
kind = "native"
description = "z"
"#,
    );
    let report = check(root, &CheckOpts::default());
    assert!(!report.ok);
    assert!(
        report
            .errors
            .iter()
            .any(|d| d.code == ErrorCode::SchemaUnsupported)
    );
}

#[test]
fn check_surfaces_invalid_namespace() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    write_manifest(
        root,
        r#"
schema = 1
[package]
namespace = "BadNs"
name = "y"
version = "0.1.0"
kind = "native"
description = "z"
"#,
    );
    let report = check(root, &CheckOpts::default());
    assert!(!report.ok);
    assert_eq!(
        report.errors[0].code,
        ErrorCode::PackageNamespaceInvalid
    );
}

#[test]
fn check_flags_no_matching_files_for_include() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    write_manifest(
        root,
        r#"
schema = 1
[package]
namespace = "x"
name = "y"
version = "0.1.0"
kind = "native"
description = "z"

[[layer]]
name = "bin"
include = ["target/release/nope"]
"#,
    );
    let report = check(root, &CheckOpts::default());
    assert!(!report.ok);
    assert!(
        report
            .errors
            .iter()
            .any(|d| d.code == ErrorCode::LayerIncludeNoMatches)
    );
}

#[test]
fn check_json_serializable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    write_manifest(
        root,
        r#"
schema = 1
[package]
namespace = "ns"
name = "x"
version = "0.1.0"
kind = "native"
description = "z"

[[layer]]
name = "bin"
include = ["target/release/nope"]
"#,
    );
    let report = check(root, &CheckOpts::default());
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["ok"], false);
    let code = json["errors"][0]["code"].as_str().unwrap();
    assert_eq!(code, "layer-include-no-matches");
    // Stable shape:
    assert!(json["errors"][0]["field"].is_string());
    assert!(json["errors"][0]["message"].is_string());
}

#[test]
fn check_passes_on_good_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::create_dir_all(root.join("target/release")).unwrap();
    fs::write(root.join("target/release/hello"), b"x").unwrap();
    write_manifest(
        root,
        r#"
schema = 1
[package]
namespace = "dragon"
name = "hello"
version = "0.1.0"
kind = "native"
description = "A greeting"

[[layer]]
name = "bin"
include = ["target/release/hello"]
"#,
    );
    let report = check(root, &CheckOpts::default());
    assert!(report.ok, "errors = {:?}", report.errors);
}
