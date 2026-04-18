use std::fs;

use camino::Utf8Path;
use elu_author::walk::{walk_layer, WalkOpts};
use elu_manifest::Layer;

fn layer_src(include: &[&str]) -> Layer {
    Layer {
        diff_id: None,
        size: None,
        name: None,
        include: include.iter().map(|s| s.to_string()).collect(),
        exclude: vec![],
        strip: None,
        place: None,
        mode: None,
        follow_symlinks: false,
    }
}

#[test]
fn walk_matches_include_and_sorts_output() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/b.rs"), b"b").unwrap();
    fs::write(root.join("src/a.rs"), b"a").unwrap();
    fs::write(root.join("README.md"), b"r").unwrap();

    let layer = layer_src(&["src/*.rs"]);
    let resolved = walk_layer(root, &layer, &WalkOpts::default()).unwrap();

    let paths: Vec<String> = resolved.iter().map(|e| e.layer_path.clone()).collect();
    assert_eq!(paths, vec!["src/a.rs".to_string(), "src/b.rs".to_string()]);
}

#[test]
fn walk_strip_and_place_compose() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::create_dir_all(root.join("target/release")).unwrap();
    fs::write(root.join("target/release/hello"), b"x").unwrap();

    let mut layer = layer_src(&["target/release/hello"]);
    layer.strip = Some("target/release/".into());
    layer.place = Some("bin/".into());

    let resolved = walk_layer(root, &layer, &WalkOpts::default()).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].layer_path, "bin/hello");
}

#[test]
fn walk_exclude_filters_out() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("dist/a.js"), b"1").unwrap();
    fs::write(root.join("dist/a.js.map"), b"2").unwrap();

    let mut layer = layer_src(&["dist/**"]);
    layer.exclude = vec!["**/*.map".into()];

    let resolved = walk_layer(root, &layer, &WalkOpts::default()).unwrap();
    let paths: Vec<String> = resolved.iter().map(|e| e.layer_path.clone()).collect();
    assert_eq!(paths, vec!["dist/a.js".to_string()]);
}

#[test]
fn walk_rejects_absolute_include() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();

    let layer = layer_src(&["/etc/passwd"]);
    let err = walk_layer(root, &layer, &WalkOpts::default()).unwrap_err();
    assert_eq!(err.code, elu_author::report::ErrorCode::LayerAbsolutePath);
}

#[test]
fn walk_rejects_parent_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();

    let layer = layer_src(&["../outside"]);
    let err = walk_layer(root, &layer, &WalkOpts::default()).unwrap_err();
    assert_eq!(err.code, elu_author::report::ErrorCode::LayerParentEscape);
}

#[test]
fn walk_rejects_invalid_glob() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();

    let layer = layer_src(&["src/[unclosed"]);
    let err = walk_layer(root, &layer, &WalkOpts::default()).unwrap_err();
    assert_eq!(err.code, elu_author::report::ErrorCode::GlobInvalid);
}
