use std::fs;

use camino::Utf8Path;
use elu_author::infer::infer_from_dir;

#[test]
fn infers_rust_project_with_bin_and_docs() {
    let tmp = tempfile::tempdir().unwrap();
    let src = Utf8Path::from_path(tmp.path()).unwrap();
    fs::write(
        src.join("Cargo.toml"),
        r#"[package]
name = "hello-tree"
version = "0.3.1"
edition = "2021"
"#,
    )
    .unwrap();
    fs::write(src.join("README.md"), b"# hello").unwrap();
    fs::write(src.join("CHANGELOG.md"), b"# changes").unwrap();

    let rendered = infer_from_dir(src).unwrap();

    assert!(rendered.contains("\"hello-tree\""));
    assert!(rendered.contains("\"0.3.1\""));
    assert!(rendered.contains("\"target/release/hello-tree\""));
    assert!(rendered.contains("README.md"));
    assert!(rendered.contains("CHANGELOG.md"));
    assert!(rendered.contains("TODO"));

    // Must parse as a valid source-form manifest.
    let parsed = elu_manifest::from_toml_str(&rendered).expect("parses");
    elu_manifest::validate::validate_source(&parsed).expect("valid source-form");
}

#[test]
fn infers_node_project_with_dist_layer() {
    let tmp = tempfile::tempdir().unwrap();
    let src = Utf8Path::from_path(tmp.path()).unwrap();
    fs::write(
        src.join("package.json"),
        r#"{"name":"@acme/widget","version":"2.0.1"}"#,
    )
    .unwrap();
    let rendered = infer_from_dir(src).unwrap();
    assert!(rendered.contains("\"widget\""));
    assert!(rendered.contains("\"2.0.1\""));
    assert!(rendered.contains("\"dist/**\""));
    let parsed = elu_manifest::from_toml_str(&rendered).expect("parses");
    elu_manifest::validate::validate_source(&parsed).expect("valid");
}

#[test]
fn empty_dir_falls_back_with_todos() {
    let tmp = tempfile::tempdir().unwrap();
    let src = Utf8Path::from_path(tmp.path()).unwrap();
    let rendered = infer_from_dir(src).unwrap();
    assert!(rendered.contains("TODO"));
    assert!(rendered.contains("schema = 1"));
}
