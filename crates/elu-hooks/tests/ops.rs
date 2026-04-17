use std::collections::BTreeMap;

use camino::Utf8Path;
use elu_hooks::{HookError, HookMode, HookRunner, HookStats, PackageContext};
use elu_manifest::HookOp;

fn staging() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn staging_path(dir: &tempfile::TempDir) -> &Utf8Path {
    Utf8Path::from_path(dir.path()).unwrap()
}

fn pkg() -> PackageContext<'static> {
    PackageContext {
        namespace: "core",
        name: "hello",
        version: "1.2.3",
        kind: "bin",
    }
}

fn run_ops(staging: &Utf8Path, ops: &[HookOp]) -> Result<HookStats, HookError> {
    let pkg = pkg();
    let runner = HookRunner::new(staging, &pkg, HookMode::Safe);
    runner.run(ops)
}

// --- HookRunner ---

#[test]
fn off_mode_skips_all_ops() {
    let dir = staging();
    let s = staging_path(&dir);
    let pkg = pkg();
    let runner = HookRunner::new(s, &pkg, HookMode::Off);
    let stats = runner
        .run(&[HookOp::Mkdir {
            path: "should-not-exist".into(),
            mode: None,
            parents: false,
        }])
        .unwrap();
    assert_eq!(stats.ops_run, 0);
    assert!(!s.join("should-not-exist").exists());
}

#[test]
fn empty_ops_returns_zero_stats() {
    let dir = staging();
    let s = staging_path(&dir);
    let stats = run_ops(s, &[]).unwrap();
    assert_eq!(stats.ops_run, 0);
}

// --- chmod ---

#[cfg(unix)]
#[test]
fn chmod_happy_path() {
    use std::os::unix::fs::PermissionsExt;

    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("script.sh").as_std_path(), b"#!/bin/sh").unwrap();

    run_ops(
        s,
        &[HookOp::Chmod {
            paths: vec!["script.sh".into()],
            mode: "+x".into(),
        }],
    )
    .unwrap();

    let perms = std::fs::metadata(s.join("script.sh").as_std_path())
        .unwrap()
        .permissions()
        .mode();
    assert_ne!(perms & 0o111, 0, "execute bits should be set");
}

#[test]
fn chmod_path_escape_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    // The glob pattern itself won't match anything outside staging,
    // but let's make sure a direct .. pattern is handled safely
    let result = run_ops(
        s,
        &[HookOp::Chmod {
            paths: vec!["../etc/passwd".into()],
            mode: "0777".into(),
        }],
    );
    // glob_in_staging won't match anything outside staging, so it's a no-op
    assert!(result.is_ok());
}

// --- mkdir ---

#[test]
fn mkdir_happy_path() {
    let dir = staging();
    let s = staging_path(&dir);
    run_ops(
        s,
        &[HookOp::Mkdir {
            path: "newdir".into(),
            mode: None,
            parents: false,
        }],
    )
    .unwrap();
    assert!(s.join("newdir").is_dir());
}

#[test]
fn mkdir_parents() {
    let dir = staging();
    let s = staging_path(&dir);
    run_ops(
        s,
        &[HookOp::Mkdir {
            path: "a/b/c".into(),
            mode: None,
            parents: true,
        }],
    )
    .unwrap();
    assert!(s.join("a/b/c").is_dir());
}

#[test]
fn mkdir_already_exists_is_noop() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::create_dir(s.join("existing").as_std_path()).unwrap();
    run_ops(
        s,
        &[HookOp::Mkdir {
            path: "existing".into(),
            mode: None,
            parents: false,
        }],
    )
    .unwrap();
}

#[test]
fn mkdir_path_escape_rejected() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = run_ops(
        s,
        &[HookOp::Mkdir {
            path: "../../escape".into(),
            mode: None,
            parents: false,
        }],
    )
    .unwrap_err();
    assert!(matches!(err, HookError::Op { .. }));
}

// --- symlink ---

#[cfg(unix)]
#[test]
fn symlink_happy_path() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("target.txt").as_std_path(), b"hello").unwrap();
    run_ops(
        s,
        &[HookOp::Symlink {
            from: "link.txt".into(),
            to: "target.txt".into(),
            replace: false,
        }],
    )
    .unwrap();
    assert!(s.join("link.txt").as_std_path().read_link().is_ok());
    assert_eq!(
        std::fs::read_to_string(s.join("link.txt").as_std_path()).unwrap(),
        "hello"
    );
}

#[cfg(unix)]
#[test]
fn symlink_exists_no_replace_fails() {
    let dir = staging();
    let s = staging_path(&dir);
    std::os::unix::fs::symlink("old", s.join("link").as_std_path()).unwrap();
    let err = run_ops(
        s,
        &[HookOp::Symlink {
            from: "link".into(),
            to: "new".into(),
            replace: false,
        }],
    )
    .unwrap_err();
    assert!(matches!(err, HookError::Op { .. }));
}

#[cfg(unix)]
#[test]
fn symlink_replace_works() {
    let dir = staging();
    let s = staging_path(&dir);
    std::os::unix::fs::symlink("old", s.join("link").as_std_path()).unwrap();
    run_ops(
        s,
        &[HookOp::Symlink {
            from: "link".into(),
            to: "new".into(),
            replace: true,
        }],
    )
    .unwrap();
    let target = std::fs::read_link(s.join("link").as_std_path()).unwrap();
    assert_eq!(target.to_string_lossy(), "new");
}

// --- write ---

#[test]
fn write_happy_path() {
    let dir = staging();
    let s = staging_path(&dir);
    run_ops(
        s,
        &[HookOp::Write {
            path: "hello.txt".into(),
            content: "Hello, {package.name}!".into(),
            mode: None,
            replace: false,
        }],
    )
    .unwrap();
    let content = std::fs::read_to_string(s.join("hello.txt").as_std_path()).unwrap();
    assert_eq!(content, "Hello, hello!");
}

#[test]
fn write_exists_no_replace_fails() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("existing.txt").as_std_path(), b"old").unwrap();
    let err = run_ops(
        s,
        &[HookOp::Write {
            path: "existing.txt".into(),
            content: "new".into(),
            mode: None,
            replace: false,
        }],
    )
    .unwrap_err();
    assert!(matches!(err, HookError::Op { .. }));
}

#[test]
fn write_replace_works() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("existing.txt").as_std_path(), b"old").unwrap();
    run_ops(
        s,
        &[HookOp::Write {
            path: "existing.txt".into(),
            content: "new".into(),
            mode: None,
            replace: true,
        }],
    )
    .unwrap();
    let content = std::fs::read_to_string(s.join("existing.txt").as_std_path()).unwrap();
    assert_eq!(content, "new");
}

#[test]
fn write_creates_parent_dirs() {
    let dir = staging();
    let s = staging_path(&dir);
    run_ops(
        s,
        &[HookOp::Write {
            path: "sub/dir/file.txt".into(),
            content: "nested".into(),
            mode: None,
            replace: false,
        }],
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(s.join("sub/dir/file.txt").as_std_path()).unwrap(),
        "nested"
    );
}

// --- template ---

#[test]
fn template_happy_path() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(
        s.join("config.tpl").as_std_path(),
        b"name={package.name}\nprefix={var.prefix}",
    )
    .unwrap();

    let mut vars = BTreeMap::new();
    vars.insert("prefix".to_string(), "/usr/local".to_string());

    let pkg = pkg();
    let runner = HookRunner::new(s, &pkg, HookMode::Safe);
    runner
        .run(&[HookOp::Template {
            input: "config.tpl".into(),
            output: "config.txt".into(),
            vars,
            mode: None,
        }])
        .unwrap();

    let content = std::fs::read_to_string(s.join("config.txt").as_std_path()).unwrap();
    assert_eq!(content, "name=hello\nprefix=/usr/local");
}

// --- copy ---

#[test]
fn copy_single_file() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("src.txt").as_std_path(), b"content").unwrap();
    run_ops(
        s,
        &[HookOp::Copy {
            from: "src.txt".into(),
            to: "dst.txt".into(),
        }],
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(s.join("dst.txt").as_std_path()).unwrap(),
        "content"
    );
    // Source still exists
    assert!(s.join("src.txt").exists());
}

#[test]
fn copy_glob_to_dir() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::create_dir(s.join("src").as_std_path()).unwrap();
    std::fs::write(s.join("src/a.txt").as_std_path(), b"a").unwrap();
    std::fs::write(s.join("src/b.txt").as_std_path(), b"b").unwrap();
    std::fs::create_dir(s.join("dest").as_std_path()).unwrap();

    run_ops(
        s,
        &[HookOp::Copy {
            from: "src/*.txt".into(),
            to: "dest/".into(),
        }],
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(s.join("dest/a.txt").as_std_path()).unwrap(),
        "a"
    );
    assert_eq!(
        std::fs::read_to_string(s.join("dest/b.txt").as_std_path()).unwrap(),
        "b"
    );
}

// --- move ---

#[test]
fn move_single_file() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("src.txt").as_std_path(), b"content").unwrap();
    run_ops(
        s,
        &[HookOp::Move {
            from: "src.txt".into(),
            to: "dst.txt".into(),
        }],
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(s.join("dst.txt").as_std_path()).unwrap(),
        "content"
    );
    // Source is gone
    assert!(!s.join("src.txt").exists());
}

// --- delete ---

#[test]
fn delete_file() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("junk.txt").as_std_path(), b"junk").unwrap();
    run_ops(
        s,
        &[HookOp::Delete {
            paths: vec!["junk.txt".into()],
        }],
    )
    .unwrap();
    assert!(!s.join("junk.txt").exists());
}

#[test]
fn delete_dir_recursive() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::create_dir_all(s.join("sub/deep").as_std_path()).unwrap();
    std::fs::write(s.join("sub/deep/file.txt").as_std_path(), b"x").unwrap();
    run_ops(
        s,
        &[HookOp::Delete {
            paths: vec!["sub".into()],
        }],
    )
    .unwrap();
    assert!(!s.join("sub").exists());
}

#[test]
fn delete_no_match_is_noop() {
    let dir = staging();
    let s = staging_path(&dir);
    run_ops(
        s,
        &[HookOp::Delete {
            paths: vec!["nonexistent*".into()],
        }],
    )
    .unwrap();
}

// --- index ---

#[test]
fn index_sha256_list_deterministic() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::create_dir_all(s.join("data").as_std_path()).unwrap();
    std::fs::write(s.join("data/a.txt").as_std_path(), b"alpha").unwrap();
    std::fs::write(s.join("data/b.txt").as_std_path(), b"beta").unwrap();

    run_ops(
        s,
        &[HookOp::Index {
            root: "data".into(),
            output: "index1.txt".into(),
            format: elu_manifest::IndexFormat::Sha256List,
        }],
    )
    .unwrap();

    run_ops(
        s,
        &[HookOp::Index {
            root: "data".into(),
            output: "index2.txt".into(),
            format: elu_manifest::IndexFormat::Sha256List,
        }],
    )
    .unwrap();

    let idx1 = std::fs::read_to_string(s.join("index1.txt").as_std_path()).unwrap();
    let idx2 = std::fs::read_to_string(s.join("index2.txt").as_std_path()).unwrap();
    assert_eq!(idx1, idx2, "index must be deterministic");
    assert!(idx1.contains("a.txt"));
    assert!(idx1.contains("b.txt"));
    assert!(idx1.contains("sha256:"));
}

#[test]
fn index_json_format() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::create_dir_all(s.join("data").as_std_path()).unwrap();
    std::fs::write(s.join("data/file.txt").as_std_path(), b"content").unwrap();

    run_ops(
        s,
        &[HookOp::Index {
            root: "data".into(),
            output: "index.json".into(),
            format: elu_manifest::IndexFormat::Json,
        }],
    )
    .unwrap();

    let content = std::fs::read_to_string(s.join("index.json").as_std_path()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 1);
}

// --- patch ---

#[test]
fn patch_inline_happy_path() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("file.txt").as_std_path(), "line1\nline2\nline3\n").unwrap();

    let diff = "\
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line1
-line2
+line2_modified
 line3
";
    run_ops(
        s,
        &[HookOp::Patch {
            file: "file.txt".into(),
            source: elu_manifest::PatchSource::Inline {
                diff: diff.to_string(),
            },
            fuzz: false,
        }],
    )
    .unwrap();

    let content = std::fs::read_to_string(s.join("file.txt").as_std_path()).unwrap();
    assert!(content.contains("line2_modified"));
    assert!(!content.contains("\nline2\n"));
}

#[test]
fn patch_from_file() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("file.txt").as_std_path(), "hello\n").unwrap();

    let diff = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-hello
+goodbye
";
    std::fs::write(s.join("fix.patch").as_std_path(), diff).unwrap();

    run_ops(
        s,
        &[HookOp::Patch {
            file: "file.txt".into(),
            source: elu_manifest::PatchSource::File {
                from: "fix.patch".into(),
            },
            fuzz: false,
        }],
    )
    .unwrap();

    let content = std::fs::read_to_string(s.join("file.txt").as_std_path()).unwrap();
    assert_eq!(content, "goodbye\n");
}

#[test]
fn patch_bad_diff_fails() {
    let dir = staging();
    let s = staging_path(&dir);
    std::fs::write(s.join("file.txt").as_std_path(), "original\n").unwrap();

    let diff = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-wrong_original
+replacement
";
    let err = run_ops(
        s,
        &[HookOp::Patch {
            file: "file.txt".into(),
            source: elu_manifest::PatchSource::Inline {
                diff: diff.to_string(),
            },
            fuzz: false,
        }],
    )
    .unwrap_err();
    assert!(matches!(err, HookError::Op { .. }));
}

// --- multi-op sequence ---

#[test]
fn multi_op_sequence() {
    let dir = staging();
    let s = staging_path(&dir);
    let stats = run_ops(
        s,
        &[
            HookOp::Mkdir {
                path: "bin".into(),
                mode: None,
                parents: false,
            },
            HookOp::Write {
                path: "bin/hello.sh".into(),
                content: "#!/bin/sh\necho {package.name}".into(),
                mode: None,
                replace: false,
            },
        ],
    )
    .unwrap();
    assert_eq!(stats.ops_run, 2);
    assert!(s.join("bin/hello.sh").exists());
    let content = std::fs::read_to_string(s.join("bin/hello.sh").as_std_path()).unwrap();
    assert_eq!(content, "#!/bin/sh\necho hello");
}

// --- op error reports index ---

#[test]
fn op_error_includes_index() {
    let dir = staging();
    let s = staging_path(&dir);
    let err = run_ops(
        s,
        &[
            HookOp::Mkdir {
                path: "ok".into(),
                mode: None,
                parents: false,
            },
            HookOp::Write {
                path: "../../escape".into(),
                content: "bad".into(),
                mode: None,
                replace: false,
            },
        ],
    )
    .unwrap_err();
    match err {
        HookError::Op { index, .. } => assert_eq!(index, 1),
        other => panic!("expected Op error, got {other:?}"),
    }
}
