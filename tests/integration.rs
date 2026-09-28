use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("cmdrun").unwrap()
}

fn make_config() -> (TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.toml");
    fs::write(
        &path,
        r#"
[base]
description = "Common"
commands = [
    "echo base-1",
    "echo base-2",
]

[extra]
description = "Extra block"
commands = [
    "echo extra-1",
]
"#,
    )
    .unwrap();
    (dir, path.to_string_lossy().into_owned())
}

#[test]
fn test_help_exit_zero() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Smart Command Runner"))
        .stdout(predicate::str::contains("--file"))
        .stdout(predicate::str::contains("--run"))
        .stdout(predicate::str::contains("--interactive-block"))
        .stdout(predicate::str::contains("--interactive-command"))
        .stdout(predicate::str::contains("Repository:"));
}

#[test]
fn test_version() {
    let output = bin()
        .arg("--version")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let expected = env!("CARGO_PKG_VERSION");
    assert!(text.contains(expected));
}

#[test]
fn test_list_blocks() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("base"))
        .stdout(predicate::str::contains("extra"))
        .stdout(predicate::str::contains("Common"))
        .stdout(predicate::str::contains("Extra block"));
}

#[test]
fn test_dry_run_default_no_execution() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry-run"))
        .stdout(predicate::str::contains("echo base-1"))
        .stdout(predicate::str::contains("no commands were executed"));
}

#[test]
fn test_yes_runs_all_blocks() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path, "--yes", "--no-log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("base-1"))
        .stdout(predicate::str::contains("base-2"))
        .stdout(predicate::str::contains("extra-1"))
        .stdout(predicate::str::contains("Commands run:   3"));
}

#[test]
fn test_run_specific_blocks() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path, "--run", "extra", "--yes", "--no-log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("extra-1"))
        .stdout(predicate::str::contains("Commands run:   1"));
}

#[test]
fn test_run_unknown_block_errors() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path, "--run", "nope", "--yes", "--no-log"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_yes_with_interactive_conflict() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path, "--yes", "--interactive-block"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be combined"));
}

#[test]
fn test_interactive_requires_tty() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path, "--interactive-block"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("require a TTY"));
}

#[test]
fn test_nonexistent_file_errors() {
    bin()
        .args(["--file", "/tmp/does_not_exist_xyz_12345.toml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn test_dry_run_flag_with_yes_still_dry() {
    let (_dir, path) = make_config();
    bin()
        .args(["--file", &path, "--yes", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no commands were executed"));
}
