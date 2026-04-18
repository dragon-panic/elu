use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_flag_prints_version() {
    Command::cargo_bin("elu")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("elu"));
}

#[test]
fn help_lists_all_documented_verbs() {
    let assert = Command::cargo_bin("elu").unwrap().arg("--help").assert().success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for verb in [
        "install", "add", "remove", "lock", "update", "stack", "init", "build", "check",
        "explain", "schema", "publish", "import", "search", "inspect", "audit", "policy",
        "ls", "gc", "fsck", "refs", "config", "completion",
    ] {
        assert!(out.contains(verb), "verb '{verb}' missing from --help: {out}");
    }
}

#[test]
fn missing_subcommand_exits_two() {
    Command::cargo_bin("elu").unwrap().assert().failure().code(2);
}

#[test]
fn unknown_subcommand_exits_two() {
    Command::cargo_bin("elu")
        .unwrap()
        .arg("nonexistent-verb")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn global_flags_appear_in_help() {
    let assert = Command::cargo_bin("elu").unwrap().arg("--help").assert().success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for flag in [
        "--store", "--registry", "--offline", "--locked", "--hooks", "--json",
    ] {
        assert!(out.contains(flag), "flag '{flag}' missing from --help: {out}");
    }
}
