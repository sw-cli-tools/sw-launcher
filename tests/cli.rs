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

// Graph wired up in Phase 4 step 5; no manifest in test cwd
// makes the binary fail with E0092 (file not found) rather
// than a NotImplemented error.
#[test]
fn graph_with_no_manifest_fails_cleanly() {
    cmd()
        .args(["graph", "any"])
        .assert()
        .failure()
        .stderr(contains("E0091").or(contains("E0092")));
}

// Cache subcommands wired up in Phase 4 step 2; smoke-test against
// a fresh tempdir cache so the binary exits cleanly with no
// entries.
#[test]
fn cache_list_on_empty_cache_succeeds_silently() {
    let tmp = tempfile::tempdir().expect("tempdir");
    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "list"])
        .assert()
        .success();
}

#[test]
fn cache_explain_with_unknown_prefix_fails_with_no_match() {
    let tmp = tempfile::tempdir().expect("tempdir");
    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "explain", "deadbeef"])
        .assert()
        .failure()
        .stderr(contains("no cache entry"));
}

#[test]
fn cache_clean_on_empty_cache_reports_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "clean"])
        .assert()
        .success()
        .stdout(contains("nothing to remove"));
}

// Vendor sync/status wired up in Phase 4 step 3; smoke-tested in
// tests/vendor_cli.rs against tempdir manifests. The default-path
// invocation here exits with E0092 because there's no
// sw-launch.toml in the test cwd.
#[test]
fn vendor_sync_with_no_manifest_fails_cleanly() {
    cmd()
        .args(["vendor", "sync"])
        .assert()
        .failure()
        .stderr(contains("E0091").or(contains("E0092")));
}

#[test]
fn vendor_status_with_no_lockfile_fires_e0040() {
    cmd()
        .args(["vendor", "status"])
        .assert()
        .failure()
        .stderr(contains("E0040"));
}

// Doctor wired up in Phase 4 step 4. The host environment for
// CI may or may not have cor24-run on PATH, so we only assert
// that the binary exits cleanly (0 with cor24-run present, 1
// otherwise) and that the table prints recognizable rows.
#[test]
fn doctor_runs_and_prints_recognizable_rows() {
    let assert = cmd().arg("doctor").assert();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("cor24-run"),
        "doctor should always print a cor24-run row; got:\n{stdout}"
    );
    assert!(stdout.contains("cache"));
}

#[test]
fn build_dispatches_assembler_pcode_and_binary_layers() {
    // Gated on both cor24-run AND pa24r being on PATH. The fixture
    // has one layer of each kind; the test asserts all three
    // produced artifact paths appear on stdout.
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let on_path = |bin: &str| std::env::split_paths(&path_var).any(|p| p.join(bin).is_file());
    if !on_path("cor24-run") || !on_path("pa24r") {
        eprintln!("cor24-run or pa24r not on PATH; skipping dispatch test");
        return;
    }
    let mut fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture.push("tests/fixtures/scenario_b_dispatch/sw-launch.toml");
    cmd()
        .args(["build", "dispatch", "--config", fixture.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("asm_part"))
        .stdout(contains("pcode_part"))
        .stdout(contains("binary_part"));
}

#[test]
fn unknown_subcommand_fails_cleanly() {
    cmd()
        .arg("teleport")
        .assert()
        .failure()
        .stderr(contains("teleport").or(contains("unrecognized")));
}
