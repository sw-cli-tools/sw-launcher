//! Negative tests: one per implementable v1.2 validation rule.
//! Plus a positive test that the canonical Scenario A fixture
//! validates cleanly.

use camino::Utf8PathBuf;
use sw_launcher::config::Config;
use sw_launcher::error::ErrorCode;
use sw_launcher::validate::{Severity, validate};

fn parse(s: &str) -> Config {
    Config::from_toml_str(s).expect("toml should parse")
}

fn run(cfg: &Config, scen: &str) -> Vec<sw_launcher::validate::Diagnostic> {
    match validate(cfg, scen) {
        Ok(d) | Err(d) => d,
    }
}

fn has_code(d: &[sw_launcher::validate::Diagnostic], code: u16) -> bool {
    d.iter().any(|x| x.code == ErrorCode(code))
}

#[test]
fn e0001_scenario_with_no_layers() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.empty]
        target = "cor24"
        layers = []
        entry  = "0x000000"
        [scenarios.empty.run]
        max_cycles = 1000
        "#,
    );
    let d = run(&cfg, "empty");
    assert!(has_code(&d, 1), "expected E0001, got {:?}", d);
}

#[test]
fn e0002_undeclared_layer_referenced() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["ghost"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 2), "expected E0002, got {:?}", d);
}

#[test]
fn e0005_kind_load_method_mismatch() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["bad"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.bad]
        kind = "binary"
        [layers.bad.load]
        method = "uart"
        max_bytes = 1024
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 5), "expected E0005, got {:?}", d);
}

#[test]
fn e0008_scenario_missing_halt_and_max_cycles() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["a"]
        entry  = "0x000000"
        [layers.a]
        kind = "binary"
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 8), "expected E0008, got {:?}", d);
}

#[test]
fn e0011_segment_outside_sram() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["bad"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.bad]
        kind = "binary"
        [[layers.bad.segments]]
        name = "bad_heap"
        kind = "heap"
        embedded = false
        size = "0x010000"
        [layers.bad.segments.load]
        method = "memory"
        address = "0xFF0000"
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 11), "expected E0011, got {:?}", d);
}

#[test]
fn e0013_zero_size_heap_segment() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["x"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.x]
        kind = "binary"
        [[layers.x.segments]]
        kind = "heap"
        embedded = false
        size = "0x000000"
        [layers.x.segments.load]
        method = "memory"
        address = "0x010000"
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 13), "expected E0013, got {:?}", d);
}

#[test]
fn e0014_embedded_without_symbol() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["x"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.x]
        kind = "assembler"
        [[layers.x.segments]]
        kind = "heap"
        embedded = true
        size = "0x001000"
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 14), "expected E0014, got {:?}", d);
}

#[test]
fn e0015_reserved_without_address() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["x"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.x]
        kind = "data"
        [[layers.x.segments]]
        kind = "heap"
        embedded = false
        size = "0x001000"
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 15), "expected E0015, got {:?}", d);
}

#[test]
fn e0016_self_address_outside_segment() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["x"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.x]
        kind = "binary"
        patches = [
          { target = "0x000A12", value = "self.address" },
        ]
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 16), "expected E0016, got {:?}", d);
}

#[test]
fn e0030_heap_over_32k_without_justification() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["fat"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.fat]
        kind = "binary"
        [[layers.fat.segments]]
        name = "h"
        kind = "heap"
        embedded = false
        size = "0x010000"
        [layers.fat.segments.load]
        method = "memory"
        address = "0x010000"
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 30), "expected E0030, got {:?}", d);
}

#[test]
fn e0031_undeclared_memory_profile() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        memory_profile = "ghost-profile"
        layers = ["a"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1000
        [layers.a]
        kind = "binary"
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 31), "expected E0031, got {:?}", d);
}

#[test]
fn e0006_unknown_referenced_layer_with_did_you_mean() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["pcode_vm", "app"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1
        [layers.pcode_vm]
        kind = "assembler"
        exports = { symbols = ["code_ptr"] }
        [layers.app]
        kind = "binary"
        patches = [
          { target = "pcode_vmm.code_ptr", value = "0x010000" },
        ]
        "#,
    );
    let d = run(&cfg, "demo");
    assert!(has_code(&d, 6), "expected E0006, got {:?}", d);
    let msg = d.iter().find(|x| x.code == ErrorCode(6)).unwrap();
    assert!(
        msg.hint
            .as_deref()
            .map(|h| h.contains("pcode_vm"))
            .unwrap_or(false),
        "expected did-you-mean hint pointing at pcode_vm, got {:?}",
        msg.hint
    );
}

#[test]
fn e0006_layer_has_no_exports_block() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["a", "b"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1
        [layers.a]
        kind = "assembler"
        [layers.b]
        kind = "binary"
        patches = [
          { target = "a.code_ptr", value = "0x010000" },
        ]
        "#,
    );
    let d = run(&cfg, "demo");
    let msg = d.iter().find(|x| x.code == ErrorCode(6)).unwrap();
    assert!(
        msg.hint
            .as_deref()
            .map(|h| h.contains("exports"))
            .unwrap_or(false),
        "expected exports hint, got {:?}",
        msg.hint
    );
}

#[test]
fn e0006_symbol_not_in_exports_with_did_you_mean() {
    let cfg = parse(
        r#"
        schema_version = 1
        [project]
        name = "x"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["vm", "app"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1
        [layers.vm]
        kind = "assembler"
        exports = { symbols = ["code_ptr", "heap_top"] }
        [layers.app]
        kind = "binary"
        patches = [
          { target = "vm.code_ptrr", value = "0x010000" },
        ]
        "#,
    );
    let d = run(&cfg, "demo");
    let msg = d.iter().find(|x| x.code == ErrorCode(6)).unwrap();
    assert!(
        msg.hint
            .as_deref()
            .map(|h| h.contains("code_ptr"))
            .unwrap_or(false),
        "expected did-you-mean hint pointing at code_ptr, got {:?}",
        msg.hint
    );
}

#[test]
fn scenario_a_fixture_validates_cleanly() {
    let mut p = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/scenario_a.toml");
    let cfg = Config::from_path(&p).expect("scenario_a parses");
    let result = validate(&cfg, "echo");
    let warnings_only =
        matches!(&result, Ok(ws) if ws.iter().all(|d| d.severity != Severity::Error));
    if !warnings_only {
        let diags = result.unwrap_or_else(|d| d);
        panic!(
            "scenario_a should validate; got: {}",
            diags
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("\n--\n")
        );
    }
}
