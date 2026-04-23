use elu_manifest::types::{Hook, Layer, Manifest, Metadata, Package};
use elu_outputs::qcow2::base::{OsBase, parse_os_base};
use elu_outputs::OutputError;
use semver::Version;

fn manifest(kind: &str, metadata_toml: &str) -> Manifest {
    let metadata: toml::value::Table = toml::from_str(metadata_toml).unwrap();
    Manifest {
        schema: 1,
        package: Package {
            namespace: "debian".into(),
            name: "bookworm-minbase".into(),
            version: Version::new(1, 0, 0),
            kind: kind.into(),
            description: "".into(),
            tags: vec![],
        },
        layers: Vec::<Layer>::new(),
        dependencies: vec![],
        hook: Hook { ops: vec![] },
        metadata: Metadata(metadata),
    }
}

#[test]
fn parses_os_base_metadata_block() {
    let m = manifest(
        "os-base",
        r#"
[os-base]
arch     = "amd64"
kernel   = "linux-image-amd64"
init     = "systemd"
finalize = ["update-initramfs", "-u"]
"#,
    );
    let OsBase {
        arch,
        kernel,
        init,
        finalize,
    } = parse_os_base(&m).unwrap();
    assert_eq!(arch, "amd64");
    assert_eq!(kernel, "linux-image-amd64");
    assert_eq!(init, "systemd");
    assert_eq!(finalize, vec!["update-initramfs", "-u"]);
}

#[test]
fn empty_finalize_allowed() {
    let m = manifest(
        "os-base",
        r#"
[os-base]
arch     = "amd64"
kernel   = "linux-image-amd64"
init     = "systemd"
finalize = []
"#,
    );
    let ob = parse_os_base(&m).unwrap();
    assert!(ob.finalize.is_empty());
}

#[test]
fn wrong_kind_rejected() {
    let m = manifest(
        "native",
        r#"
[os-base]
arch = "amd64"
kernel = "x"
init = "systemd"
finalize = []
"#,
    );
    let err = parse_os_base(&m).unwrap_err();
    assert!(matches!(err, OutputError::Base(_)), "got {err:?}");
    let msg = format!("{err}");
    assert!(msg.contains("kind"), "expected kind error, got: {msg}");
}

#[test]
fn missing_metadata_table_rejected() {
    let m = manifest("os-base", "");
    let err = parse_os_base(&m).unwrap_err();
    assert!(matches!(err, OutputError::Base(_)), "got {err:?}");
}

#[test]
fn missing_required_field_rejected() {
    let m = manifest(
        "os-base",
        r#"
[os-base]
arch = "amd64"
# no kernel
init = "systemd"
"#,
    );
    let err = parse_os_base(&m).unwrap_err();
    assert!(matches!(err, OutputError::Base(_)), "got {err:?}");
    let msg = format!("{err}");
    assert!(msg.contains("kernel"));
}
