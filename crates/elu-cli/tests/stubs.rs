use assert_cmd::Command;

fn assert_stub(args: &[&str], dep_marker: &str) {
    let out = Command::cargo_bin("elu").unwrap().args(args).output().unwrap();
    assert!(!out.status.success(), "{args:?} should fail until dispatch lands");
    assert_eq!(
        out.status.code(),
        Some(1),
        "{args:?} should exit 1 (generic) — got: {:?}",
        out.status.code()
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("not yet implemented"),
        "{args:?} stub message missing 'not yet implemented': {stderr}"
    );
    assert!(
        stderr.contains(dep_marker),
        "{args:?} stub message missing depends-on marker '{dep_marker}': {stderr}"
    );
}

#[test]
fn stack_resolution_error_when_ref_not_in_store() {
    // No matching ref in a fresh store; resolver fails fast.
    let store = tempfile::TempDir::new().unwrap();
    let out_dir = tempfile::TempDir::new().unwrap();
    let out = out_dir.path().join("stacked");
    let result = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--store",
            store.path().to_str().unwrap(),
            "stack",
            "ns/pkg@1.0.0",
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .failure();
    let code = result.get_output().status.code();
    assert_eq!(
        code,
        Some(3),
        "should exit 3 (resolution); got {code:?}"
    );
}

#[test]
fn audit_is_stub() {
    assert_stub(&["audit"], "WKIW.wX0h");
}

#[test]
fn policy_show_is_stub() {
    assert_stub(&["policy", "show"], "policy");
}

#[test]
fn policy_check_is_stub() {
    assert_stub(&["policy", "check", "ns/pkg"], "policy");
}

#[test]
fn install_help_lists_output_flag() {
    let assert = Command::cargo_bin("elu")
        .unwrap()
        .args(["install", "--help"])
        .assert()
        .success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(s.contains("-o") || s.contains("--out"), "install --help missing -o/--out: {s}");
}

#[test]
fn stack_requires_output_flag_exits_two() {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["stack", "ns/pkg"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn audit_help_lists_fail_on_flag() {
    let assert = Command::cargo_bin("elu")
        .unwrap()
        .args(["audit", "--help"])
        .assert()
        .success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(s.contains("--fail-on"), "audit --help missing --fail-on: {s}");
}

#[test]
fn policy_help_lists_subcommands() {
    let assert = Command::cargo_bin("elu")
        .unwrap()
        .args(["policy", "--help"])
        .assert()
        .success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for sub in ["show", "check", "allow", "deny", "revoke", "set"] {
        assert!(s.contains(sub), "policy --help missing {sub}: {s}");
    }
}
