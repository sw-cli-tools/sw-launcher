//! Integration tests for `sw-launch graph`, shipped in Phase 4
//! step 5.

use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn cmd(dir: &Path) -> Command {
    let mut c = Command::cargo_bin("sw-launch").expect("binary built");
    c.current_dir(dir);
    c
}

fn write_two_layer_manifest(dir: &Path) {
    // Mimics scenario_b: a runtime + a p-code app, with a
    // cross-layer code_ptr patch from the app onto the
    // runtime's exported symbol.
    let toml = r#"schema_version = 1

[project]
name = "graph-test"

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
layers = ["pcode_vm", "pcode_app"]
entry  = "0x000000"

[scenarios.demo.run]
mode       = "batch"
timeout_ms = 1000
max_cycles = 1000
halt_on    = "monitor-exit"

[layers.pcode_vm]
kind = "binary"
input = "pvm.bin"
[layers.pcode_vm.exports]
symbols = ["code_ptr"]
[layers.pcode_vm.load]
method  = "memory"
address = "0x000000"

[layers.pcode_app]
kind = "binary"
input = "app.p24m"
patches = [
  { target = "pcode_vm.code_ptr", value = "0x010000" },
]
[layers.pcode_app.load]
method  = "memory"
address = "0x010000"
"#;
    std::fs::write(dir.join("sw-launch.toml"), toml).unwrap();
    std::fs::write(dir.join("pvm.bin"), b"runtime").unwrap();
    std::fs::write(dir.join("app.p24m"), b"app").unwrap();
}

#[test]
fn graph_text_renders_in_declaration_order() {
    let tmp = TempDir::new().unwrap();
    write_two_layer_manifest(tmp.path());
    cmd(tmp.path())
        .args(["graph", "demo"])
        .assert()
        .success()
        .stdout(contains("demo"))
        .stdout(contains("pcode_vm"))
        .stdout(contains("pcode_app"));
}

#[test]
fn graph_json_parses_and_includes_expected_keys() {
    let tmp = TempDir::new().unwrap();
    write_two_layer_manifest(tmp.path());
    let assert = cmd(tmp.path()).args(["graph", "demo", "--json"]).assert();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.starts_with('{'),
        "JSON object expected; got {stdout}"
    );
    for needle in [
        r#""scenario":"demo""#,
        r#""target":"cor24""#,
        r#""name":"pcode_vm""#,
        r#""name":"pcode_app""#,
        r#""depends_on":"#,
    ] {
        assert!(stdout.contains(needle), "missing `{needle}` in:\n{stdout}");
    }
}

#[test]
fn graph_includes_cross_layer_patch_edge() {
    let tmp = TempDir::new().unwrap();
    write_two_layer_manifest(tmp.path());
    let assert = cmd(tmp.path()).args(["graph", "demo", "--json"]).assert();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    // pcode_app patches pcode_vm.code_ptr -> the JSON layer
    // entry for pcode_app must list pcode_vm in depends_on.
    let app_idx = stdout
        .find(r#""name":"pcode_app""#)
        .expect("pcode_app node");
    let app_chunk = &stdout[app_idx..];
    let deps_idx = app_chunk.find(r#""depends_on":["#).expect("depends_on key");
    let deps_end = app_chunk[deps_idx..].find(']').expect("depends_on close");
    let deps = &app_chunk[deps_idx..deps_idx + deps_end + 1];
    assert!(
        deps.contains(r#""pcode_vm""#),
        "pcode_app should depend on pcode_vm; got {deps}"
    );
}

#[test]
fn graph_unknown_scenario_fails_cleanly() {
    let tmp = TempDir::new().unwrap();
    write_two_layer_manifest(tmp.path());
    cmd(tmp.path())
        .args(["graph", "no-such-scenario"])
        .assert()
        .failure()
        .stderr(contains("not declared"));
}
