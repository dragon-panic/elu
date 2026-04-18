use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn import_help_lists_kinds_and_options() {
    let assert = Command::cargo_bin("elu").unwrap().args(["import", "--help"]).assert().success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for f in ["--closure", "--dist", "--target", "--version", "apt", "npm", "pip"] {
        assert!(s.contains(f), "import --help missing {f}: {s}");
    }
}

#[test]
fn import_unknown_kind_exits_two() {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["import", "rubygems", "rails"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn import_apt_with_corrupt_cache_returns_failure_not_panic() {
    // Pre-populate the cache with bogus bytes so the importer never hits the
    // network. The dispatch wiring is what we're exercising — the importer's
    // own error path is what we expect.
    let store = TempDir::new().unwrap();
    let cache = store.path().join("cache/apt/curl/8.0.0");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, b"not a real deb").unwrap();

    let out = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "import",
            "apt",
            "curl",
            "--version",
            "8.0.0",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected failure, got: {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // Must be reaching the real importer — error must mention archive/parse,
    // not "not yet wired" / "not yet implemented".
    assert!(
        !stderr.contains("not yet wired") && !stderr.contains("not yet implemented"),
        "import dispatch is still a stub: {stderr}"
    );
    assert!(
        stderr.contains("archive") || stderr.contains("ar ") || stderr.contains("deb"),
        "expected importer error message in stderr: {stderr}"
    );
}

#[test]
fn import_missing_name_exits_two() {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["import", "apt"])
        .assert()
        .failure()
        .code(2);
}
