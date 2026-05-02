//! End-to-end test for Scenario C: pvm.bin@0 + ocaml.p24m@0x040000
//! with code_ptr + heap_limit patches read from the OCaml repo's
//! sidecar files; OCaml source delivered via UART with EOT.
//!
//! Skipped (with a one-line note on stderr) when:
//!   - cor24-run is not on PATH
//!   - ~/github/sw-embed/sw-cor24-ocaml/build/ doesn't have
//!     pvm.bin / ocaml.p24m / code_ptr_addr.txt /
//!     heap_limit_addr.txt
//!
//! pa24r and p24-load are NOT required: the test uses
//! `kind = "binary"` for both pvm and ocaml_interp, since the
//! OCaml repo already ships pre-built artifacts under build/.

use std::path::{Path, PathBuf};

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

fn ocaml_build_dir() -> Option<PathBuf> {
    let mut p = home()?;
    p.push("github/sw-embed/sw-cor24-ocaml/build");
    let needed = [
        "pvm.bin",
        "ocaml.p24m",
        "code_ptr_addr.txt",
        "heap_limit_addr.txt",
    ];
    for f in needed {
        if !p.join(f).is_file() {
            return None;
        }
    }
    Some(p)
}

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/scenario_c");
    p
}

fn tmp_dir(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut p = std::env::temp_dir();
    p.push(format!("sw-launcher-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_toml(dest: &Path, ocaml_build: &Path, source: &Path, heap_size: &str) {
    let pvm = ocaml_build.join("pvm.bin");
    let ocaml_p24m = ocaml_build.join("ocaml.p24m");
    let code_ptr = ocaml_build.join("code_ptr_addr.txt");
    let heap_limit = ocaml_build.join("heap_limit_addr.txt");
    let toml = format!(
        r#"schema_version = 1

[project]
name = "scenario-c"

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

[scenarios.nested-demo]
target = "cor24"
layers = ["pcode_vm", "ocaml_interp", "ocaml_source"]
entry  = "0x000000"

[scenarios.nested-demo.run]
mode       = "batch"
timeout_ms = 60_000
max_cycles = 3_000_000_000
halt_on    = "monitor-exit"

[scenarios.nested-demo.expect]
uart_contains = ["3"]

[layers.pcode_vm]
kind = "binary"
input = "{pvm}"
[layers.pcode_vm.load]
method  = "memory"
address = "0x000000"

[layers.ocaml_interp]
kind = "binary"
input = "{ocaml_p24m}"
patches = [
  {{ target = "sidecar:{code_ptr}",   value = "0x040000" }},
  {{ target = "sidecar:{heap_limit}", value = "0x03F000" }},
]
[layers.ocaml_interp.load]
method  = "memory"
address = "0x040000"

[layers.ocaml_source]
kind = "text"
input = "{source}"
[layers.ocaml_source.load]
method     = "uart"
max_bytes  = {heap_size}
terminator = "EOT"
"#,
        pvm = pvm.display(),
        ocaml_p24m = ocaml_p24m.display(),
        code_ptr = code_ptr.display(),
        heap_limit = heap_limit.display(),
        source = source.display(),
        heap_size = heap_size,
    );
    std::fs::write(dest, toml).unwrap();
}

fn skip_unless_ready() -> Option<(PathBuf, PathBuf)> {
    if !on_path("cor24-run") {
        eprintln!("cor24-run not on PATH; skipping scenario_c test");
        return None;
    }
    let Some(build) = ocaml_build_dir() else {
        eprintln!(
            "sw-cor24-ocaml/build/ artifacts not present; skipping scenario_c test \
             (run `just build` in that repo)"
        );
        return None;
    };
    Some((tmp_dir("scenario-c"), build))
}

#[test]
fn sw_launch_run_ocaml_demo_against_real_tools() {
    let Some((out, build)) = skip_unless_ready() else {
        return;
    };
    let toml = out.join("sw-launch.toml");
    let demo = fixture_dir().join("demo.ml");
    write_toml(&toml, &build, &demo, "16384");
    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .args(["run", "nested-demo", "--config", toml.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("3"));
}

#[test]
fn run_with_oversized_source_fails_with_e0004() {
    let Some((out, build)) = skip_unless_ready() else {
        return;
    };
    // Generate a source file larger than max_bytes (set to 64
    // for this test) so check_uart_size fires E0004.
    let big = out.join("big.ml");
    let mut blob = String::new();
    for _ in 0..200 {
        blob.push_str("(* padding *)\n");
    }
    std::fs::write(&big, &blob).unwrap();
    assert!(
        blob.len() > 64,
        "fixture must exceed max_bytes for the test"
    );
    let toml = out.join("sw-launch-bad.toml");
    write_toml(&toml, &build, &big, "64");
    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .args(["check", "nested-demo", "--config", toml.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("E0004").or(contains("max_bytes")));
}
