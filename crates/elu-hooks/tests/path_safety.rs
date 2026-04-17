use camino::Utf8Path;
use elu_hooks::HookError;

fn staging() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn staging_path(dir: &tempfile::TempDir) -> &Utf8Path {
    Utf8Path::from_path(dir.path()).unwrap()
}

#[test]
fn resolve_normal_path() {
    let dir = staging();
    let s = staging_path(&dir);
    let result = elu_hooks::path::resolve_in_staging(s, "bin/hello").unwrap();
    assert_eq!(result, s.join("bin/hello"));
}

#[test]
fn resolve_dotdot_escape_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = elu_hooks::path::resolve_in_staging(s, "../../etc/passwd").unwrap_err();
    assert!(matches!(err, HookError::PathEscape(_)));
}

#[test]
fn resolve_absolute_path_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = elu_hooks::path::resolve_in_staging(s, "/etc/passwd").unwrap_err();
    assert!(matches!(err, HookError::PathEscape(_)));
}

#[test]
fn resolve_nul_byte_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = elu_hooks::path::resolve_in_staging(s, "hello\0world").unwrap_err();
    assert!(matches!(err, HookError::PathEscape(_)));
}

#[test]
fn resolve_windows_drive_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = elu_hooks::path::resolve_in_staging(s, "C:\\Windows\\System32").unwrap_err();
    assert!(matches!(err, HookError::PathEscape(_)));
}

#[test]
fn resolve_dotdot_within_staging_ok() {
    let dir = staging();
    let s = staging_path(&dir);
    // a/b/../c normalizes to a/c, which is still inside staging
    let result = elu_hooks::path::resolve_in_staging(s, "a/b/../c").unwrap();
    assert_eq!(result, s.join("a/c"));
}

#[test]
fn resolve_leading_dotdot_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = elu_hooks::path::resolve_in_staging(s, "../sibling").unwrap_err();
    assert!(matches!(err, HookError::PathEscape(_)));
}

#[test]
fn resolve_deep_dotdot_escape_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    // a/../../outside escapes: a -> pop a -> pop staging -> outside
    let err = elu_hooks::path::resolve_in_staging(s, "a/../../outside").unwrap_err();
    assert!(matches!(err, HookError::PathEscape(_)));
}

#[test]
fn resolve_dot_and_empty_ignored() {
    let dir = staging();
    let s = staging_path(&dir);
    let result = elu_hooks::path::resolve_in_staging(s, "./a/./b").unwrap();
    assert_eq!(result, s.join("a/b"));
}

#[cfg(unix)]
#[test]
fn verify_symlink_escape_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    // Create a symlink inside staging that points outside
    std::fs::create_dir_all(s.join("sub").as_std_path()).unwrap();
    std::os::unix::fs::symlink("/tmp", s.join("sub/escape").as_std_path()).unwrap();
    let target = s.join("sub/escape/somefile");
    let err = elu_hooks::path::verify_under_staging(s, &target);
    assert!(err.is_err());
}

#[test]
fn glob_in_staging_matches_files() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::create_dir_all(s.join("bin").as_std_path()).unwrap();
    std::fs::write(s.join("bin/hello").as_std_path(), b"#!/bin/sh").unwrap();
    std::fs::write(s.join("bin/world").as_std_path(), b"#!/bin/sh").unwrap();
    std::fs::write(s.join("readme.txt").as_std_path(), b"hi").unwrap();

    let matches = elu_hooks::path::glob_in_staging(s, "bin/*").unwrap();
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().any(|p| p.as_str().ends_with("bin/hello")));
    assert!(matches.iter().any(|p| p.as_str().ends_with("bin/world")));
}

#[test]
fn glob_no_matches_returns_empty() {
    let dir = staging();
    let s = staging_path(&dir);
    let matches = elu_hooks::path::glob_in_staging(s, "nonexistent/*.txt").unwrap();
    assert!(matches.is_empty());
}

#[test]
fn resolve_backslash_escape_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = elu_hooks::path::resolve_in_staging(s, "\\etc\\passwd").unwrap_err();
    assert!(matches!(err, HookError::PathEscape(_)));
}
