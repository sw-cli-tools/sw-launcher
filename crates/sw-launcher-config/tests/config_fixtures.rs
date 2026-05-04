//! Integration tests that load full sw-launch.toml fixtures and
//! verify the typed Config tree faithfully represents schema v1.2.
//!
//! These tests are *parsing* tests, not *validation* tests. Cross-
//! field validation (overlap, kind/load compatibility, profile
//! resolution) lands in step 006-scenario-validate.

use camino::Utf8PathBuf;
use sw_launcher_config::{Config, LoadMethod, MemRegionKind, SegmentKind};

fn fixture_path(name: &str) -> Utf8PathBuf {
    // Phase 5 step 1: this test moved into sw-launcher-config
    // but the fixture .toml files stay in the main crate's
    // tests/fixtures/ where other binaries (manifest_unit,
    // scenario_a) also reference them.
    let mut p = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/fixtures");
    p.push(name);
    p
}

#[test]
fn scenario_a_parses_end_to_end() {
    let path = fixture_path("scenario_a.toml");
    let cfg = Config::from_path(&path).expect("scenario_a fixture should parse");

    assert_eq!(cfg.schema_version, 1);
    assert_eq!(cfg.project.name, "echo-demo");
    assert_eq!(cfg.project.default_target.as_deref(), Some("cor24"));
    assert_eq!(cfg.project.default_scenario.as_deref(), Some("echo"));

    let cor24 = cfg.targets.get("cor24").expect("cor24 target declared");
    assert_eq!(cor24.kind, "emulator");
    assert_eq!(cor24.word_bits, 24);
    assert_eq!(cor24.regions.sram.start.as_u32().unwrap(), 0x000000);
    assert_eq!(cor24.regions.sram.end.as_u32().unwrap(), 0x0FFFFF);
    assert_eq!(cor24.regions.ebr_stack.role.as_deref(), Some("hw-stack"));

    let profile = cfg
        .memory_profiles
        .get("compiled-app")
        .expect("compiled-app profile declared");
    assert_eq!(profile.partitions.len(), 3);
    assert_eq!(profile.partitions[0].name, "code");
    assert_eq!(profile.partitions[0].regions[0].kind, MemRegionKind::Code);
    let budget = profile.budget.as_ref().expect("compiled-app has a budget");
    assert_eq!(budget.heap_max.as_u32().unwrap(), 0x004000);
    assert!(budget.justification_required);

    let scen = cfg.scenarios.get("echo").expect("echo scenario declared");
    assert_eq!(scen.target, "cor24");
    assert_eq!(scen.memory_profile.as_deref(), Some("compiled-app"));
    assert_eq!(scen.layers.len(), 2);
    let run = scen.run.as_ref().expect("run config present");
    assert_eq!(run.mode.as_deref(), Some("batch"));
    assert_eq!(run.timeout_ms, Some(2000));

    let expect = scen.expect.as_ref().expect("expect config present");
    assert_eq!(expect.uart_contains, vec!["abc!".to_string()]);
    assert_eq!(expect.exit_code, Some(0));

    let layer = cfg
        .layers
        .get("echo_program")
        .expect("echo_program layer declared");
    assert_eq!(layer.kind, "assembler");
    assert_eq!(layer.tool.as_deref(), Some("assembler"));
    let load = layer.load.as_ref().expect("load present");
    assert_eq!(load.method, LoadMethod::Memory);
    assert_eq!(load.address.as_ref().unwrap().as_u32().unwrap(), 0x000000);

    let stdin = cfg.layers.get("stdin_data").expect("stdin_data declared");
    let load = stdin.load.as_ref().expect("uart load present");
    assert_eq!(load.method, LoadMethod::Uart);
    assert_eq!(load.max_bytes, Some(1024));
}

#[test]
fn multi_layer_embedded_parses_end_to_end() {
    let path = fixture_path("multi_layer_embedded.toml");
    let cfg = Config::from_path(&path).expect("multi_layer fixture should parse");

    let pvm = cfg.layers.get("pcode_vm").expect("pcode_vm declared");
    let exports = pvm.exports.as_ref().expect("exports present");
    assert!(exports.symbols.contains(&"code_ptr".to_string()));
    assert!(exports.symbols.contains(&"eval_stack".to_string()));
    assert_eq!(pvm.segments.len(), 3);
    assert_eq!(pvm.segments[0].name.as_deref(), Some("eval_stack"));
    assert_eq!(pvm.segments[0].kind, SegmentKind::Stack);
    assert!(pvm.segments[0].embedded.unwrap_or(false));
    assert_eq!(pvm.segments[0].symbol.as_deref(), Some("eval_stack"));

    let app = cfg.layers.get("pcode_app").expect("pcode_app declared");
    assert_eq!(app.patches.len(), 1);
    assert_eq!(app.patches[0].target, "pcode_vm.code_ptr");
}
