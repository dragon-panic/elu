use std::fs;

use camino::Utf8Path;
use elu_author::init::{init_builtin, BuiltinKind, InitOpts};
use elu_manifest::validate::validate_source;

fn assert_parses_and_validates(root: &Utf8Path) {
    let src = fs::read_to_string(root.join("elu.toml")).unwrap();
    let parsed = elu_manifest::from_toml_str(&src).expect("TOML parses");
    validate_source(&parsed).expect("source-form valid");
}

fn opts(name: &str, ns: &str) -> InitOpts {
    InitOpts {
        name: name.to_string(),
        namespace: ns.to_string(),
    }
}

#[test]
fn init_native_writes_parseable_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    init_builtin(root, BuiltinKind::Native, &opts("hello", "dragon")).unwrap();

    assert!(root.join("elu.toml").exists());
    assert_parses_and_validates(root);
}

#[test]
fn init_ox_skill_writes_parseable_toml_with_bin_layer() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    init_builtin(root, BuiltinKind::OxSkill, &opts("my-skill", "ox-community")).unwrap();
    assert_parses_and_validates(root);
    let src = fs::read_to_string(root.join("elu.toml")).unwrap();
    assert!(src.contains("kind"));
    assert!(src.contains("ox-skill"));
    assert!(src.contains("bin"));
}

#[test]
fn init_ox_persona_writes_parseable_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    init_builtin(root, BuiltinKind::OxPersona, &opts("careful", "dragon")).unwrap();
    assert_parses_and_validates(root);
    let src = fs::read_to_string(root.join("elu.toml")).unwrap();
    assert!(src.contains("ox-persona"));
}

#[test]
fn init_ox_runtime_writes_parseable_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    init_builtin(root, BuiltinKind::OxRuntime, &opts("claude", "dragon")).unwrap();
    assert_parses_and_validates(root);
    let src = fs::read_to_string(root.join("elu.toml")).unwrap();
    assert!(src.contains("ox-runtime"));
}

#[test]
fn init_refuses_to_overwrite_existing_elu_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    fs::write(root.join("elu.toml"), b"existing").unwrap();
    let err = init_builtin(root, BuiltinKind::Native, &opts("x", "y")).unwrap_err();
    assert!(err.message.contains("exists"));
}
