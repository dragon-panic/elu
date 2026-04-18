use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn bash_completion_emits_function_definition() {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["completion", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_elu()"));
}

#[test]
fn zsh_completion_emits_compdef() {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["completion", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef elu"));
}

#[test]
fn fish_completion_emits_complete_directive() {
    Command::cargo_bin("elu")
        .unwrap()
        .args(["completion", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -c elu"));
}

#[test]
fn missing_shell_argument_exits_two() {
    Command::cargo_bin("elu")
        .unwrap()
        .arg("completion")
        .assert()
        .failure()
        .code(2);
}
