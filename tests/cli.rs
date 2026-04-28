//! Integration tests for the sw-launch binary's CLI surface.
//!
//! Step 003-scaffold-cli only stubs the subcommands; every action
//! returns NotImplemented. These tests assert the contract surface,
//! not the eventual behavior.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn cmd() -> Command {
    Command::cargo_bin("sw-launch").expect("binary built")
}

#[test]
fn version_flag_prints_cargo_version() {
    let pkg_version = env!("CARGO_PKG_VERSION");
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("sw-launch"))
        .stdout(contains(pkg_version));
}

#[test]
fn help_flag_lists_every_top_level_subcommand() {
    let assert = cmd().arg("--help").assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in [
        "run", "build", "check", "graph", "cache", "vendor", "doctor",
    ] {
        assert!(
            stdout.contains(sub),
            "expected --help to mention `{sub}`, got:\n{stdout}"
        );
    }
}

#[test]
fn run_with_no_scenario_fails_and_mentions_scenario() {
    // clap's value name renders as `SCENARIO`; accept either case.
    cmd()
        .arg("run")
        .assert()
        .failure()
        .stderr(contains("scenario").or(contains("SCENARIO")));
}

#[test]
fn run_with_missing_config_fails_cleanly() {
    // run is wired up as of step 009; without a config file it
    // should surface an I/O / cli error code, not the
    // "not yet implemented" sentinel.
    cmd()
        .args(["run", "any", "--config", "no-such-file.toml"])
        .assert()
        .failure()
        .stderr(contains("E0091").or(contains("E0092")));
}

#[test]
fn build_with_missing_config_fails_cleanly() {
    // build is wired up as of step 007; without a config file it
    // should surface an I/O / cli error code, not the "not yet
    // implemented" sentinel.
    cmd()
        .args(["build", "any", "--config", "no-such-file.toml"])
        .assert()
        .failure()
        .stderr(contains("E0091").or(contains("E0092")));
}

#[test]
fn check_with_missing_config_fails_cleanly() {
    // check is wired up as of step 006; running in a directory
    // without sw-launch.toml should surface an I/O / cli error.
    cmd()
        .args(["check", "any", "--config", "no-such-file.toml"])
        .assert()
        .failure()
        .stderr(contains("E0091").or(contains("E0092")));
}

#[test]
fn graph_returns_not_implemented() {
    cmd()
        .args(["graph", "any"])
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}

#[test]
fn cache_list_returns_not_implemented() {
    cmd()
        .args(["cache", "list"])
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}

#[test]
fn cache_explain_returns_not_implemented() {
    cmd()
        .args(["cache", "explain", "scenario-name"])
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}

#[test]
fn cache_clean_returns_not_implemented() {
    cmd()
        .args(["cache", "clean"])
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}

#[test]
fn vendor_sync_returns_not_implemented() {
    cmd()
        .args(["vendor", "sync"])
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}

#[test]
fn vendor_status_returns_not_implemented() {
    cmd()
        .args(["vendor", "status"])
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}

#[test]
fn doctor_returns_not_implemented() {
    cmd()
        .arg("doctor")
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}

#[test]
fn unknown_subcommand_fails_cleanly() {
    cmd()
        .arg("teleport")
        .assert()
        .failure()
        .stderr(contains("teleport").or(contains("unrecognized")));
}
