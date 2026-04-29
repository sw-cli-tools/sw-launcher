//! End-to-end test for Scenario B: pvm.bin@0 + hello.p24m@0x010000
//! with `code_ptr` patch. Skipped (with a one-line note on stderr)
//! if any of the required tools / source files are missing.
//!
//! The test pre-links the p-code app via `pa24r` + `p24-load`
//! before invoking `sw-launch`. Phase 3 will add pcode-linker
//! integration to the launcher itself; for Phase 2 we use the
//! `kind = "pcode-image"` pass-through path.

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn on_path(bin: &str) -> bool {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path_var).any(|p| p.join(bin).is_file())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn pvm_s() -> Option<PathBuf> {
    let mut p = home()?;
    p.push("github/sw-embed/sw-cor24-pcode/vm/pvm.s");
    if p.is_file() { Some(p) } else { None }
}

/// pa24r and p24-load aren't always on PATH; check the sibling
/// release build location too.
fn find_pcode_tool(bin: &str) -> Option<PathBuf> {
    if on_path(bin) {
        let path_var = std::env::var_os("PATH").unwrap_or_default();
        if let Some(p) = std::env::split_paths(&path_var).find(|p| p.join(bin).is_file()) {
            return Some(p.join(bin));
        }
    }
    let mut sibling = home()?;
    sibling.push("github/sw-embed/sw-cor24-pcode/target/release");
    sibling.push(bin);
    if sibling.is_file() {
        Some(sibling)
    } else {
        None
    }
}

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/scenario_b");
    p
}

fn tmp_dir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("sw-launcher-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Pre-build pvm.bin (cor24-run --assemble), hello.p24 (pa24r),
/// hello.p24m (p24-load --load-addr 0x010000) into `out`.
/// Returns the resolved code_ptr address.
fn prebuild(out: &Path, pa24r: &Path, p24load: &Path) -> u32 {
    let pvm_src = pvm_s().expect("pvm.s exists per gate above");
    let pvm_bin = out.join("pvm.bin");
    let pvm_lst = out.join("pvm.lst");
    let s1 = StdCommand::new("cor24-run")
        .args(["--assemble"])
        .args([pvm_src.as_path(), &pvm_bin, &pvm_lst])
        .status()
        .unwrap();
    assert!(s1.success(), "cor24-run --assemble pvm.s failed");
    let hello_p24 = out.join("hello.p24");
    let s2 = StdCommand::new(pa24r)
        .arg(fixture_dir().join("hello.spc"))
        .arg("-o")
        .arg(&hello_p24)
        .status()
        .unwrap();
    assert!(s2.success(), "pa24r hello.spc failed");
    let hello_p24m = out.join("hello.p24m");
    let s3 = StdCommand::new(p24load)
        .arg(&hello_p24)
        .args(["--load-addr", "0x010000", "-o"])
        .arg(&hello_p24m)
        .status()
        .unwrap();
    assert!(s3.success(), "p24-load hello.p24 failed");
    // Parse code_ptr address from pvm.lst. The fixture's listing
    // typically has two `code_ptr:` labels (placeholder + init);
    // the parser overwrites with the latest, which is what we want.
    let lst = std::fs::read_to_string(&pvm_lst).unwrap();
    parse_symbol_addr(&lst, "code_ptr").expect("code_ptr in pvm.lst")
}

/// Find the address of the **last** occurrence of `sym:` followed
/// by a data line. The listing emitted by `cor24-run --assemble`
/// can have the same label more than once (placeholder slot, then
/// initialized slot for `code_ptr`); we want the initialized one.
fn parse_symbol_addr(text: &str, sym: &str) -> Option<u32> {
    let label = format!("{sym}:");
    let mut last: Option<u32> = None;
    let mut after_label = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == label {
            after_label = true;
            continue;
        }
        if after_label
            && let Some((addr_hex, _)) = trimmed.split_once(':')
            && addr_hex.chars().all(|c| c.is_ascii_hexdigit())
            && let Ok(a) = u32::from_str_radix(addr_hex, 16)
        {
            last = Some(a);
            after_label = false;
        }
    }
    last
}

fn write_toml(dest: &Path, pvm_bin: &Path, hello_p24m: &Path, code_ptr: u32) {
    let toml = format!(
        r#"schema_version = 1

[project]
name = "scenario-b"

[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"

[targets.cor24.regions]
sram      = {{ start = "0x000000", end = "0x0FFFFF" }}
ebr_stack = {{ start = "0xFEEC00", end = "0xFEF7FF", role = "hw-stack" }}
mmio      = {{ start = "0xFF0000", end = "0xFFFFFF" }}

[scenarios.pcode-hello]
target = "cor24"
layers = ["pcode_vm", "pcode_app"]
entry  = "0x000000"

[scenarios.pcode-hello.run]
mode       = "batch"
timeout_ms = 5000
max_cycles = 5000000

[scenarios.pcode-hello.expect]
uart_contains = ["Hello"]

[layers.pcode_vm]
kind = "binary"
input = "{pvm_bin}"
exports = {{ symbols = ["code_ptr"] }}
[layers.pcode_vm.load]
method  = "memory"
address = "0x000000"

[layers.pcode_app]
kind = "pcode-image"
input = "{hello_p24m}"
patches = [
  {{ target = "0x{code_ptr:06X}", value = "pcode_app.address" }},
]
[layers.pcode_app.load]
method  = "memory"
address = "0x010000"
"#,
        pvm_bin = pvm_bin.display(),
        hello_p24m = hello_p24m.display(),
        code_ptr = code_ptr,
    );
    std::fs::write(dest, toml).unwrap();
}

fn skip_unless_ready() -> Option<(PathBuf, PathBuf, PathBuf)> {
    if !on_path("cor24-run") {
        eprintln!("cor24-run not on PATH; skipping scenario_b test");
        return None;
    }
    let pa24r = find_pcode_tool("pa24r")?;
    let p24load = find_pcode_tool("p24-load")?;
    let _pvm = pvm_s()?;
    let out = tmp_dir("scenario-b");
    Some((out, pa24r, p24load))
}

#[test]
fn sw_launch_run_pcode_hello_against_real_tools() {
    let Some((out, pa24r, p24load)) = skip_unless_ready() else {
        eprintln!("scenario_b prerequisites missing; skipping");
        return;
    };
    let code_ptr = prebuild(&out, &pa24r, &p24load);
    let toml = out.join("sw-launch.toml");
    write_toml(
        &toml,
        &out.join("pvm.bin"),
        &out.join("hello.p24m"),
        code_ptr,
    );
    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .args(["run", "pcode-hello", "--config", toml.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("Hello"));
}

#[test]
fn run_with_misaligned_patch_fails_visibly() {
    let Some((out, pa24r, p24load)) = skip_unless_ready() else {
        eprintln!("scenario_b prerequisites missing; skipping");
        return;
    };
    let _ = prebuild(&out, &pa24r, &p24load);
    let toml = out.join("sw-launch-bad.toml");
    // Patch a deliberately wrong address (0x0FFF00) so code_ptr
    // never gets the bytecode pointer it needs. Run still
    // happens; UART output won't contain "Hello".
    write_toml(
        &toml,
        &out.join("pvm.bin"),
        &out.join("hello.p24m"),
        0x0FFF00,
    );
    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .args(["run", "pcode-hello", "--config", toml.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("expectation mismatch").or(contains("E0091")));
}
