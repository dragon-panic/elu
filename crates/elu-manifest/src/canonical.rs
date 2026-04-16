use serde::Serialize;
use serde_json::Value;

use crate::types::Manifest;

/// Serialize a manifest to canonical JSON.
///
/// Keys are sorted at every level, no insignificant whitespace,
/// empty arrays/objects omitted, numbers are integers.
pub fn to_canonical_json(m: &Manifest) -> Vec<u8> {
    let value = serde_json::to_value(m).expect("manifest must serialize to JSON");
    let normalized = normalize(value);
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::new(&mut buf);
    normalized.serialize(&mut ser).expect("normalized value must serialize");
    buf
}

/// Recursively normalize a JSON value:
/// - Objects: sort keys, omit keys with empty array/object values, recurse.
/// - Arrays: recurse into each element, drop empty arrays.
/// - Primitives: pass through.
fn normalize(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            for key in keys {
                let v = normalize(map[&key].clone());
                if is_empty_collection(&v) {
                    continue;
                }
                sorted.insert(key, v);
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(normalize).collect())
        }
        other => other,
    }
}

fn is_empty_collection(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}
