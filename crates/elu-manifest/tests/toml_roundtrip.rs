use elu_manifest::{from_toml_str, to_toml_string, HookOp, Manifest, VersionSpec};

const MINIMAL_STORED: &str = r#"
schema = 1

[package]
namespace = "dragon"
name = "hello-tree"
version = "0.1.0"
kind = "native"
description = "An example package containing a greeting file"

[[layer]]
diff_id = "sha256:d2c4000000000000000000000000000000000000000000000000000000000000"
size = 42
"#;

#[test]
fn parse_minimal_stored_manifest() {
    let m: Manifest = from_toml_str(MINIMAL_STORED).unwrap();
    assert_eq!(m.schema, 1);
    assert_eq!(m.package.namespace, "dragon");
    assert_eq!(m.package.name, "hello-tree");
    assert_eq!(m.package.version.to_string(), "0.1.0");
    assert_eq!(m.package.kind, "native");
    assert_eq!(
        m.package.description,
        "An example package containing a greeting file"
    );
    assert_eq!(m.layers.len(), 1);
    assert!(m.layers[0].is_stored_form());
    assert!(!m.layers[0].is_source_form());
    assert_eq!(m.layers[0].size, Some(42));
    assert!(m.dependencies.is_empty());
    assert!(m.hook.is_empty());
    assert!(m.metadata.is_empty());
}

#[test]
fn toml_roundtrip_minimal_stored() {
    let m1 = from_toml_str(MINIMAL_STORED).unwrap();
    let toml_out = to_toml_string(&m1).unwrap();
    let m2 = from_toml_str(&toml_out).unwrap();
    assert_eq!(m1, m2);
}

const FULL_STORED: &str = r#"
schema = 1

[package]
namespace = "ox-community"
name = "postgres-query"
version = "0.3.0"
kind = "ox-skill"
description = "Query PostgreSQL databases, inspect schemas, explain plans"
tags = ["database", "postgresql", "observability"]

[[layer]]
diff_id = "sha256:8f7a1c2e4d000000000000000000000000000000000000000000000000000000"
size = 18432
name = "bin"

[[layer]]
diff_id = "sha256:3b9e0a77f1000000000000000000000000000000000000000000000000000000"
size = 512
name = "docs"

[[dependency]]
ref = "ox-community/shell"
version = "^1.0"

[[dependency]]
ref = "ox-community/base"

[[hook.op]]
type = "chmod"
paths = ["bin/*"]
mode = "+x"

[[hook.op]]
type = "write"
path = "etc/version"
content = "0.3.0\n"

[[hook.op]]
type = "mkdir"
path = "var/log"
parents = true

[[hook.op]]
type = "symlink"
from = "bin/psql-query"
to = "bin/pq"

[[hook.op]]
type = "copy"
from = "etc/config.default"
to = "etc/config"

[[hook.op]]
type = "delete"
paths = ["tmp/build-artifacts"]

[[hook.op]]
type = "index"
root = "share/docs"
output = "share/docs/index.json"
format = "json"

[metadata]
homepage = "https://github.com/ox-community/postgres-query"

[metadata.ox]
requires = { bins = ["psql"] }
"#;

#[test]
fn parse_full_stored_manifest() {
    let m = from_toml_str(FULL_STORED).unwrap();
    assert_eq!(m.package.namespace, "ox-community");
    assert_eq!(m.package.tags, vec!["database", "postgresql", "observability"]);
    assert_eq!(m.layers.len(), 2);
    assert_eq!(m.layers[0].name, Some("bin".to_string()));
    assert_eq!(m.layers[1].name, Some("docs".to_string()));

    assert_eq!(m.dependencies.len(), 2);
    assert_eq!(m.dependencies[0].reference.as_str(), "ox-community/shell");
    assert!(matches!(&m.dependencies[0].version, VersionSpec::Range(_)));
    // Second dependency has no version, defaults to Any
    assert!(matches!(&m.dependencies[1].version, VersionSpec::Any));

    assert_eq!(m.hook.ops.len(), 7);
    assert!(matches!(&m.hook.ops[0], HookOp::Chmod { paths, mode } if paths == &["bin/*"] && mode == "+x"));
    assert!(matches!(&m.hook.ops[1], HookOp::Write { path, .. } if path == "etc/version"));
    assert!(matches!(&m.hook.ops[2], HookOp::Mkdir { path, parents, .. } if path == "var/log" && *parents));
    assert!(matches!(&m.hook.ops[3], HookOp::Symlink { from, to, .. } if from == "bin/psql-query" && to == "bin/pq"));
    assert!(matches!(&m.hook.ops[4], HookOp::Copy { from, to } if from == "etc/config.default" && to == "etc/config"));
    assert!(matches!(&m.hook.ops[5], HookOp::Delete { paths } if paths == &["tmp/build-artifacts"]));
    assert!(matches!(&m.hook.ops[6], HookOp::Index { format: elu_manifest::IndexFormat::Json, .. }));

    assert!(!m.metadata.is_empty());
    assert!(m.metadata.0.contains_key("homepage"));
    assert!(m.metadata.0.contains_key("ox"));
}

#[test]
fn toml_roundtrip_full_stored() {
    let m1 = from_toml_str(FULL_STORED).unwrap();
    let toml_out = to_toml_string(&m1).unwrap();
    let m2 = from_toml_str(&toml_out).unwrap();
    assert_eq!(m1, m2);
}

#[test]
fn parse_dependency_pinned_hash() {
    let toml = r#"
schema = 1

[package]
namespace = "test"
name = "pinned"
version = "1.0.0"
kind = "native"
description = "Test pinned dependency"

[[dependency]]
ref = "other/pkg"
version = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
"#;
    let m = from_toml_str(toml).unwrap();
    assert!(matches!(&m.dependencies[0].version, VersionSpec::Pinned(_)));
}

#[test]
fn parse_source_form_layers() {
    let toml = r#"
schema = 1

[package]
namespace = "test"
name = "source-pkg"
version = "0.1.0"
kind = "native"
description = "A source form package"

[[layer]]
include = ["src/**"]
exclude = ["src/test/**"]
name = "code"
strip = "src/"
place = "lib/"
"#;
    let m = from_toml_str(toml).unwrap();
    assert_eq!(m.layers.len(), 1);
    assert!(m.layers[0].is_source_form());
    assert!(!m.layers[0].is_stored_form());
    assert_eq!(m.layers[0].include, vec!["src/**"]);
    assert_eq!(m.layers[0].exclude, vec!["src/test/**"]);
    assert_eq!(m.layers[0].strip, Some("src/".to_string()));
    assert_eq!(m.layers[0].place, Some("lib/".to_string()));
}
