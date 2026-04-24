use assert_cmd::Command;

#[test]
fn search_help_lists_filter_flags() {
    let assert = Command::cargo_bin("elu").unwrap().args(["search", "--help"]).assert().success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for f in ["--kind", "--tag", "--namespace"] {
        assert!(s.contains(f), "search --help missing {f}: {s}");
    }
}

#[test]
fn search_against_unreachable_registry_exits_four() {
    // 127.0.0.1:1 is reliably refused; this validates network-error → exit 4
    // (CliError::Network) without needing a real server.
    let out = Command::cargo_bin("elu")
        .unwrap()
        .args([
            "--registry",
            "http://127.0.0.1:1",
            "search",
            "anything",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn search_requires_registry_or_uses_default() {
    // No --registry, no env: must produce a clear usage error rather than panic.
    let out = Command::cargo_bin("elu")
        .unwrap()
        .env_remove("ELU_REGISTRY")
        .args(["search", "anything"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_ne!(out.status.code(), Some(0));
    assert!(out.status.code().unwrap() != 101, "panic, not handled error");
}

#[test]
fn publish_help_lists_reference_arg() {
    let assert = Command::cargo_bin("elu").unwrap().args(["publish", "--help"]).assert().success();
    let s = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(s.contains("REFERENCE"), "publish --help missing REFERENCE: {s}");
}

#[test]
fn publish_requires_token() {
    // publish dispatch needs --token (or ELU_PUBLISH_TOKEN). Without it we
    // expect a clear usage error (exit 2), not a panic.
    let out = Command::cargo_bin("elu")
        .unwrap()
        .env_remove("ELU_PUBLISH_TOKEN")
        .args(["publish", "ns/pkg@1.0.0"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected usage exit 2; stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("token"), "expected token mention; stderr: {stderr}");
}
