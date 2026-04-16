use elu_manifest::{canonical::to_canonical_json, from_toml_str, manifest_hash};

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
fn canonical_json_sorted_keys_no_whitespace() {
    let m = from_toml_str(MINIMAL_STORED).unwrap();
    let json = to_canonical_json(&m);
    let s = std::str::from_utf8(&json).unwrap();

    // Verify it's valid JSON
    let val: serde_json::Value = serde_json::from_str(s).unwrap();
    assert!(val.is_object());

    // Keys must be sorted: "layer" < "package" < "schema"
    // (no "dependency", "hook", or "metadata" since they're empty — omitted)
    let obj = val.as_object().unwrap();
    let keys: Vec<&String> = obj.keys().collect();
    assert_eq!(keys, vec!["layer", "package", "schema"]);

    // Package keys sorted
    let pkg = obj["package"].as_object().unwrap();
    let pkg_keys: Vec<&String> = pkg.keys().collect();
    assert_eq!(
        pkg_keys,
        vec!["description", "kind", "name", "namespace", "version"]
    );

    // No whitespace between separators
    assert!(!s.contains(": "));
    assert!(!s.contains(", "));
    // No newlines
    assert!(!s.contains('\n'));
}

#[test]
fn canonical_json_omits_empty_collections() {
    let m = from_toml_str(MINIMAL_STORED).unwrap();
    let json = to_canonical_json(&m);
    let s = std::str::from_utf8(&json).unwrap();

    // Empty tags, dependencies, hook, metadata should all be omitted
    assert!(!s.contains("\"tags\""));
    assert!(!s.contains("\"dependency\""));
    assert!(!s.contains("\"hook\""));
    assert!(!s.contains("\"metadata\""));
}

#[test]
fn canonical_json_stable_across_calls() {
    let m = from_toml_str(MINIMAL_STORED).unwrap();
    let json1 = to_canonical_json(&m);
    let json2 = to_canonical_json(&m);
    assert_eq!(json1, json2, "canonical JSON must be deterministic");
}

/// Golden test: pin the exact bytes for a known manifest to catch drift.
#[test]
fn canonical_json_golden() {
    let m = from_toml_str(MINIMAL_STORED).unwrap();
    let json = to_canonical_json(&m);
    let s = std::str::from_utf8(&json).unwrap();
    let expected = r#"{"layer":[{"diff_id":"sha256:d2c4000000000000000000000000000000000000000000000000000000000000","size":42}],"package":{"description":"An example package containing a greeting file","kind":"native","name":"hello-tree","namespace":"dragon","version":"0.1.0"},"schema":1}"#;
    assert_eq!(s, expected, "canonical JSON golden test failed.\nGot:      {s}\nExpected: {expected}");
}

#[test]
fn manifest_hash_stable() {
    let m = from_toml_str(MINIMAL_STORED).unwrap();
    let h1 = manifest_hash(&m);
    let h2 = manifest_hash(&m);
    assert_eq!(h1, h2, "manifest hash must be stable across calls");
    // Verify it's a sha256 hash
    let s = h1.to_string();
    assert!(s.starts_with("sha256:"), "hash should have sha256 prefix");
    assert_eq!(s.len(), "sha256:".len() + 64, "sha256 hex should be 64 chars");
}

#[test]
fn manifest_hash_changes_with_content() {
    let m1 = from_toml_str(MINIMAL_STORED).unwrap();
    let mut m2 = m1.clone();
    m2.package.version = "0.2.0".parse().unwrap();
    assert_ne!(
        manifest_hash(&m1),
        manifest_hash(&m2),
        "different manifests should produce different hashes"
    );
}
