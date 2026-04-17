use std::collections::BTreeMap;

use elu_hooks::interpolate::interpolate;
use elu_hooks::{HookError, PackageContext};

fn pkg() -> PackageContext<'static> {
    PackageContext {
        namespace: "core",
        name: "hello",
        version: "1.2.3",
        kind: "bin",
    }
}

#[test]
fn package_fields_substituted() {
    let result = interpolate(
        "ns={package.namespace} name={package.name} v={package.version} k={package.kind}",
        &pkg(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(result, "ns=core name=hello v=1.2.3 k=bin");
}

#[test]
fn var_substituted() {
    let mut vars = BTreeMap::new();
    vars.insert("prefix".to_string(), "/usr/local".to_string());
    let result = interpolate("install to {var.prefix}/bin", &pkg(), &vars).unwrap();
    assert_eq!(result, "install to /usr/local/bin");
}

#[test]
fn unknown_package_field_rejected() {
    let err = interpolate("{package.unknown}", &pkg(), &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, HookError::UnknownInterpolation(_)));
}

#[test]
fn unknown_var_rejected() {
    let err = interpolate("{var.missing}", &pkg(), &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, HookError::UnknownInterpolation(_)));
}

#[test]
fn unknown_namespace_rejected() {
    let err = interpolate("{env.HOME}", &pkg(), &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, HookError::UnknownInterpolation(_)));
}

#[test]
fn unclosed_brace_rejected() {
    let err = interpolate("{package.name", &pkg(), &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, HookError::UnclosedBrace));
}

#[test]
fn no_braces_passthrough() {
    let result = interpolate("plain text", &pkg(), &BTreeMap::new()).unwrap();
    assert_eq!(result, "plain text");
}

#[test]
fn empty_string_passthrough() {
    let result = interpolate("", &pkg(), &BTreeMap::new()).unwrap();
    assert_eq!(result, "");
}

#[test]
fn multiple_substitutions() {
    let mut vars = BTreeMap::new();
    vars.insert("greeting".to_string(), "hello".to_string());
    let result = interpolate(
        "{var.greeting}, {package.name}!",
        &pkg(),
        &vars,
    )
    .unwrap();
    assert_eq!(result, "hello, hello!");
}
