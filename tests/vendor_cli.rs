//! Integration tests for `sw-launch vendor sync` / `vendor status`
//! shipped in Phase 4 step 3.
//!
//! These don't need the COR24 emulator on PATH -- the lockfile
//! subcommands only walk and hash inputs declared in
//! sw-launch.toml. We seed a tempdir with a minimal manifest +
//! a couple of fixture binaries.

use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn write_minimal_manifest(dir: &Path) {
    let toml = r#"schema_version = 1

[project]
name = "vendor-test"

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

fn cmd(dir: &Path) -> Command {
    let mut c = Command::cargo_bin("sw-launch").expect("binary built");
    c.current_dir(dir);
    c
}

#[test]
fn vendor_sync_writes_lockfile_with_one_entry() {
    let tmp = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    cmd(tmp.path())
        .args(["vendor", "sync"])
        .assert()
        .success()
        .stdout(contains("1 entries"));
    let lock = tmp.path().join("sw-launch.lock");
    assert!(lock.is_file(), "lockfile should exist");
    let body = std::fs::read_to_string(&lock).unwrap();
    assert!(body.contains("schema_version = 1"));
    assert!(body.contains(r#"key = "thing.bin""#));
    assert!(body.contains("input_sha256"));
}

#[test]
fn vendor_status_after_sync_reports_all_fresh() {
    let tmp = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    cmd(tmp.path()).args(["vendor", "sync"]).assert().success();
    cmd(tmp.path())
        .args(["vendor", "status"])
        .assert()
        .success()
        .stdout(contains("fresh"));
}

#[test]
fn vendor_status_after_tampering_reports_drifted_and_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    cmd(tmp.path()).args(["vendor", "sync"]).assert().success();
    // Mutate the input under the lockfile's feet.
    std::fs::write(tmp.path().join("thing.bin"), b"tampered").unwrap();
    cmd(tmp.path())
        .args(["vendor", "status"])
        .assert()
        .failure()
        .stdout(contains("drifted"))
        .stderr(contains("E0041"));
}

#[test]
fn run_with_drifted_lockfile_refuses_with_e0041() {
    // We don't actually run the emulator -- E0041 fires before
    // any tool is spawned. So this works without cor24-run on
    // PATH.
    let tmp = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    cmd(tmp.path()).args(["vendor", "sync"]).assert().success();
    std::fs::write(tmp.path().join("thing.bin"), b"different").unwrap();
    cmd(tmp.path())
        .args(["run", "demo"])
        .assert()
        .failure()
        .stderr(contains("E0041"));
}

#[test]
fn run_with_update_lock_skips_drift_check() {
    let tmp = TempDir::new().unwrap();
    write_minimal_manifest(tmp.path());
    cmd(tmp.path()).args(["vendor", "sync"]).assert().success();
    std::fs::write(tmp.path().join("thing.bin"), b"different").unwrap();
    // run will fail later (no cor24-run on PATH for tests, or
    // validation), but it must not fail with E0041.
    let assert = cmd(tmp.path())
        .args(["run", "demo", "--update-lock"])
        .assert();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("E0041"),
        "--update-lock should bypass drift check; got: {stderr}"
    );
}
