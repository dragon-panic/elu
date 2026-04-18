use std::collections::BTreeSet;
use std::fmt::Write;

use elu_manifest::{HookOp, Manifest};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ExplainDiff {
    pub version_change: Option<String>,
    pub dependencies_added: Vec<String>,
    pub dependencies_removed: Vec<String>,
    pub hook_ops_added: Vec<String>,
    pub hook_ops_removed: Vec<String>,
}

pub fn explain_text(m: &Manifest) -> String {
    let mut out = String::new();
    writeln!(out, "{}/{} @ {}", m.package.namespace, m.package.name, m.package.version).ok();
    writeln!(out, "  kind: {}", m.package.kind).ok();
    writeln!(out, "  description: {}", m.package.description).ok();
    if !m.package.tags.is_empty() {
        writeln!(out, "  tags: {}", m.package.tags.join(", ")).ok();
    }
    writeln!(out).ok();

    writeln!(out, "Layers ({})", m.layers.len()).ok();
    for layer in &m.layers {
        let name = layer.name.as_deref().unwrap_or("(unnamed)");
        let size = layer.size.unwrap_or(0);
        let diff_id = layer
            .diff_id
            .as_ref()
            .map(|d| d.to_string())
            .unwrap_or_else(|| "(source form)".into());
        writeln!(out, "  {name} — {size} B — {diff_id}").ok();
    }
    writeln!(out).ok();

    if !m.dependencies.is_empty() {
        writeln!(out, "Dependencies ({})", m.dependencies.len()).ok();
        for d in &m.dependencies {
            writeln!(out, "  {} {}", d.reference, format_version_spec(&d.version)).ok();
        }
        writeln!(out).ok();
    }

    if !m.hook.ops.is_empty() {
        writeln!(out, "Hook operations ({})", m.hook.ops.len()).ok();
        for (i, op) in m.hook.ops.iter().enumerate() {
            writeln!(out, "  {}. {}", i + 1, op_summary(op)).ok();
        }
    }

    out
}

pub fn diff_manifests(a: &Manifest, b: &Manifest) -> ExplainDiff {
    let version_change = if a.package.version != b.package.version {
        Some(format!("{} -> {}", a.package.version, b.package.version))
    } else {
        None
    };

    let a_deps: BTreeSet<String> = a.dependencies.iter().map(|d| d.reference.to_string()).collect();
    let b_deps: BTreeSet<String> = b.dependencies.iter().map(|d| d.reference.to_string()).collect();

    let dependencies_added: Vec<String> = b_deps.difference(&a_deps).cloned().collect();
    let dependencies_removed: Vec<String> = a_deps.difference(&b_deps).cloned().collect();

    let a_ops: Vec<String> = a.hook.ops.iter().map(op_kind).collect();
    let b_ops: Vec<String> = b.hook.ops.iter().map(op_kind).collect();

    // Multiset diff: a_ops minus common prefix
    let hook_ops_removed = multiset_diff(&a_ops, &b_ops);
    let hook_ops_added = multiset_diff(&b_ops, &a_ops);

    ExplainDiff {
        version_change,
        dependencies_added,
        dependencies_removed,
        hook_ops_added,
        hook_ops_removed,
    }
}

fn multiset_diff(left: &[String], right: &[String]) -> Vec<String> {
    let mut right_remaining: Vec<String> = right.to_vec();
    let mut out = Vec::new();
    for item in left {
        if let Some(pos) = right_remaining.iter().position(|x| x == item) {
            right_remaining.remove(pos);
        } else {
            out.push(item.clone());
        }
    }
    out
}

fn op_kind(op: &HookOp) -> String {
    match op {
        HookOp::Chmod { .. } => "chmod",
        HookOp::Mkdir { .. } => "mkdir",
        HookOp::Symlink { .. } => "symlink",
        HookOp::Write { .. } => "write",
        HookOp::Template { .. } => "template",
        HookOp::Copy { .. } => "copy",
        HookOp::Move { .. } => "move",
        HookOp::Delete { .. } => "delete",
        HookOp::Index { .. } => "index",
        HookOp::Patch { .. } => "patch",
    }
    .to_string()
}

fn op_summary(op: &HookOp) -> String {
    match op {
        HookOp::Chmod { paths, mode } => format!("chmod {} {}", paths.join(","), mode),
        HookOp::Mkdir { path, .. } => format!("mkdir {path}"),
        HookOp::Symlink { from, to, .. } => format!("symlink {from} -> {to}"),
        HookOp::Write { path, .. } => format!("write {path}"),
        HookOp::Template { input, output, .. } => format!("template {input} -> {output}"),
        HookOp::Copy { from, to } => format!("copy {from} -> {to}"),
        HookOp::Move { from, to } => format!("move {from} -> {to}"),
        HookOp::Delete { paths } => format!("delete {}", paths.join(",")),
        HookOp::Index { root, output, .. } => format!("index {root} -> {output}"),
        HookOp::Patch { file, .. } => format!("patch {file}"),
    }
}

fn format_version_spec(vs: &elu_manifest::VersionSpec) -> String {
    match vs {
        elu_manifest::VersionSpec::Range(r) => r.to_string(),
        elu_manifest::VersionSpec::Pinned(h) => h.to_string(),
        elu_manifest::VersionSpec::Any => "*".to_string(),
    }
}
