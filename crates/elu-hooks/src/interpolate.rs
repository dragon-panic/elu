use std::collections::BTreeMap;

use crate::error::HookError;
use crate::PackageContext;

/// Substitute {package.*} and {var.*} references. Unknown references
/// are a hard error.
pub fn interpolate(
    src: &str,
    pkg: &PackageContext,
    vars: &BTreeMap<String, String>,
) -> Result<String, HookError> {
    let mut result = String::with_capacity(src.len());
    let mut chars = src.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '{' {
            // Find closing brace
            let start = chars.peek().map(|(i, _)| *i);
            let mut key = String::new();
            let mut found_close = false;
            for (_, c) in chars.by_ref() {
                if c == '}' {
                    found_close = true;
                    break;
                }
                key.push(c);
            }
            if !found_close {
                return Err(HookError::UnclosedBrace);
            }
            let _ = start; // consumed above
            let value = resolve_key(&key, pkg, vars)?;
            result.push_str(&value);
        } else {
            result.push(ch);
        }
    }
    Ok(result)
}

fn resolve_key(
    key: &str,
    pkg: &PackageContext,
    vars: &BTreeMap<String, String>,
) -> Result<String, HookError> {
    if let Some(field) = key.strip_prefix("package.") {
        match field {
            "namespace" => Ok(pkg.namespace.to_string()),
            "name" => Ok(pkg.name.to_string()),
            "version" => Ok(pkg.version.to_string()),
            "kind" => Ok(pkg.kind.to_string()),
            _ => Err(HookError::UnknownInterpolation(format!("{{{key}}}"))),
        }
    } else if let Some(name) = key.strip_prefix("var.") {
        vars.get(name)
            .cloned()
            .ok_or_else(|| HookError::UnknownInterpolation(format!("{{{key}}}")))
    } else {
        Err(HookError::UnknownInterpolation(format!("{{{key}}}")))
    }
}
