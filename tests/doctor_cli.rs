//! Integration tests for `sw-launch doctor`, shipped in Phase 4
//! step 4.

use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn cmd(dir: &Path) -> Command {
    let mut c = Command::cargo_bin("sw-launch").expect("binary built");
    c.current_dir(dir);
    c
}

fn write_minimal_manifest(dir: &Path) {
    let toml = r#"schema_version = 1

[project]
name = "doctor-test"

[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"

[targets.cor24.regions]
sram      = { start = "0x000000", end = "0x0FFFFF" }
ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF", role = "hw-stack" }
mmio      = { start = "0xFF0000", end = "0xFFFFFF" }

[scenarios.demo]
target = "cor24"
layers = ["thing"]
entry  = "0x000000"

[scenarios.demo.run]
mode       = "batch"
timeout_ms = 1000
max_cycles = 1000
halt_on    = "monitor-exit"

[layers.thing]
kind = "binary"
input = "thing.bin"
[layers.thing.load]
method  = "memory"
address = "0x000000"
"#;
    std::fs::write(dir.join("sw-launch.toml"), toml).unwrap();
    std::fs::write(dir.join("thing.bin"), b"hello").unwrap();
}

#[test]
fn doctor_runs_against_clean_environment() {
    let tmp = TempDir::new().unwrap();
    let cache = TempDir::new().unwrap();
    cmd(tmp.path())
        .env("SW_LAUNCH_CACHE_DIR", cache.path())
        .arg("doctor")
        .assert()
        .stdout(contains("cor24-run"))
        .stdout(contains("cache"))
        .stdout(contains("pa24r"))
        .stdout(contains("p24-load"));
}

#[test]
fn doctor_with_drifted_lockfile_fails_with_e0041() {
    let tmp = TempDir::new().unwrap();
    let cache = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    // Sync, then tamper.
    cmd(tmp.path())
        .env("SW_LAUNCH_CACHE_DIR", cache.path())
        .args(["vendor", "sync"])
        .assert()
        .success();
    std::fs::write(tmp.path().join("thing.bin"), b"tampered").unwrap();
    cmd(tmp.path())
        .env("SW_LAUNCH_CACHE_DIR", cache.path())
        .args(["doctor", "--config", "sw-launch.toml"])
        .assert()
        .failure()
        .stdout(contains("E0041"));
}

#[test]
fn doctor_with_missing_layer_input_fails() {
    let tmp = TempDir::new().unwrap();
    let cache = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    std::fs::remove_file(tmp.path().join("thing.bin")).unwrap();
    cmd(tmp.path())
        .env("SW_LAUNCH_CACHE_DIR", cache.path())
        .args(["doctor", "--config", "sw-launch.toml"])
        .assert()
        .failure()
        .stdout(contains("FAIL"))
        .stdout(contains("thing.bin"));
}

#[test]
fn doctor_json_emits_structured_array() {
    let tmp = TempDir::new().unwrap();
    let cache = TempDir::new().unwrap();
    let assert = cmd(tmp.path())
        .env("SW_LAUNCH_CACHE_DIR", cache.path())
        .args(["doctor", "--json"])
        .assert();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.starts_with('['),
        "JSON should be an array; got:\n{stdout}"
    );
    assert!(stdout.ends_with("]\n") || stdout.ends_with(']'));
    assert!(stdout.contains(r#""name":"cor24-run""#));
    assert!(stdout.contains(r#""status":"#));
}

#[test]
fn doctor_warns_on_missing_lockfile_with_config() {
    let tmp = TempDir::new().unwrap();
    let cache = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    let assert = cmd(tmp.path())
        .env("SW_LAUNCH_CACHE_DIR", cache.path())
        .args(["doctor", "--config", "sw-launch.toml"])
        .assert();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("WARN") && stdout.contains("lockfile"),
        "doctor should warn (not fail) on missing lockfile; got:\n{stdout}"
    );
}
