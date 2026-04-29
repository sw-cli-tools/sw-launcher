//! Snapshot tests for `manifest::LoadPlan` and the cor24 argv it
//! emits. The snapshots are inline strings (no external snapshot
//! crate) so a churn in formatting fails loudly here rather than
//! later.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use sw_launcher::config::Config;
use sw_launcher::manifest::{ArtifactEntry, Artifacts, LoadPlan};
use sw_launcher::tool::Listing;

fn scenario_a_toml(input_dir: &Utf8PathBuf) -> String {
    format!(
        r#"
schema_version = 1
[project]
name = "echo-demo"
[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"
regions = {{ sram = {{ start = "0x000000", end = "0x0FFFFF" }}, ebr_stack = {{ start = "0xFEEC00", end = "0xFEF7FF" }}, mmio = {{ start = "0xFF0000", end = "0xFFFFFF" }} }}
[scenarios.echo]
target = "cor24"
layers = ["echo_program", "stdin_data"]
entry  = "0x000000"
[scenarios.echo.run]
max_cycles = 200000
[layers.echo_program]
kind = "assembler"
input = "{input_dir}/echo.s"
artifact = "echo.bin"
[layers.echo_program.load]
method = "memory"
address = "0x000000"
[layers.stdin_data]
kind = "data"
input = "{input_dir}/echo-input.txt"
[layers.stdin_data.load]
method = "uart"
max_bytes = 1024
terminator = "EOT"
"#,
        input_dir = input_dir
    )
}

fn parse(s: &str) -> Config {
    Config::from_toml_str(s).expect("toml parses")
}

fn artifacts_for_echo(input_dir: &Utf8PathBuf) -> Artifacts {
    let mut by_layer = BTreeMap::new();
    by_layer.insert(
        "echo_program".to_string(),
        ArtifactEntry {
            artifact: input_dir.join("echo.bin"),
            listing: Listing::default(),
        },
    );
    Artifacts { by_layer }
}

fn fixtures_dir() -> Utf8PathBuf {
    let mut p = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p
}

#[test]
fn scenario_a_load_plan_has_expected_shape() {
    let dir = fixtures_dir();
    let cfg = parse(&scenario_a_toml(&dir));
    let arts = artifacts_for_echo(&dir);
    let plan = LoadPlan::build(&cfg, "echo", &arts).expect("plan builds");

    assert_eq!(plan.entry, 0x000000);
    assert_eq!(plan.memory_loads.len(), 1);
    assert_eq!(plan.memory_loads[0].layer, "echo_program");
    assert_eq!(plan.memory_loads[0].address, 0x000000);
    assert_eq!(plan.uart.len(), 1);
    assert_eq!(plan.uart[0].layer, "stdin_data");
    assert_eq!(plan.uart[0].terminator, Some(0x04));
    assert!(plan.patches.is_empty());
    assert!(plan.segments.is_empty());
}

#[test]
fn scenario_a_argv_matches_golden() {
    let dir = fixtures_dir();
    let cfg = parse(&scenario_a_toml(&dir));
    let arts = artifacts_for_echo(&dir);
    let plan = LoadPlan::build(&cfg, "echo", &arts).unwrap();
    let argv = plan.cor24_argv();
    let bin = dir.join("echo.bin");
    let expected: Vec<String> = vec![
        "--load-binary".into(),
        format!("{bin}@0x000000"),
        "--entry".into(),
        "0x000000".into(),
        "--uart-input".into(),
        "abc!\n\u{04}".into(),
    ];
    assert_eq!(argv, expected, "argv:\n{argv:#?}\nexpected:\n{expected:#?}");
}

const SCENARIO_A_WITH_EMBEDDED_TOML: &str = r#"
schema_version = 1
[project]
name = "with-embedded"
[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"
regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
[scenarios.echo]
target = "cor24"
layers = ["pvm"]
entry  = "0x000000"
[scenarios.echo.run]
max_cycles = 100000
[layers.pvm]
kind = "assembler"
input = "pvm.s"
artifact = "pvm.bin"
[layers.pvm.load]
method = "memory"
address = "0x000000"
[[layers.pvm.segments]]
name = "eval_stack"
kind = "stack"
embedded = true
symbol = "eval_stack"
size = "0x000400"
[[layers.pvm.segments]]
name = "call_stack"
kind = "stack"
embedded = true
symbol = "call_stack"
size = "0x000800"
[[layers.pvm.segments]]
name = "heap_seg"
kind = "heap"
embedded = true
symbol = "heap_seg"
size = "0x002000"
"#;

#[test]
fn embedded_segments_contribute_to_segments_but_not_loads() {
    // Synthesize a fake listing with the three embedded labels at
    // believable offsets.
    let lst_text = "                    _start:\n\
                    0000: 00 00 00       .word 0\n\
                                        eval_stack:\n\
                    0010: 00 00 00 00    .skip 1024\n\
                                        call_stack:\n\
                    0410: 00             .skip 2048\n\
                                        heap_seg:\n\
                    0C10: 00             .skip 8192\n";
    let listing = Listing::parse(lst_text);
    assert_eq!(listing.resolve("eval_stack"), Some(0x0010));
    assert_eq!(listing.resolve("call_stack"), Some(0x0410));
    assert_eq!(listing.resolve("heap_seg"), Some(0x0C10));

    let mut by_layer = BTreeMap::new();
    by_layer.insert(
        "pvm".to_string(),
        ArtifactEntry {
            artifact: fixtures_dir().join("pvm.bin"),
            listing,
        },
    );
    let arts = Artifacts { by_layer };
    let cfg = parse(SCENARIO_A_WITH_EMBEDDED_TOML);
    let plan = LoadPlan::build(&cfg, "echo", &arts).unwrap();

    // Memory loads: only the layer's bin (one); embedded segments
    // do NOT add --load-binary entries.
    assert_eq!(plan.memory_loads.len(), 1);
    let argv = plan.cor24_argv();
    let load_count = argv.iter().filter(|s| s == &"--load-binary").count();
    assert_eq!(load_count, 1, "embedded segments must not emit loads");

    // segments: three embedded ranges, addresses resolved through
    // the listing, sizes from TOML.
    assert_eq!(plan.segments.len(), 3);
    assert_eq!(plan.segments[0].name.as_deref(), Some("eval_stack"));
    assert_eq!(plan.segments[0].start, 0x0010);
    assert_eq!(plan.segments[0].end, 0x0010 + 0x0400);
    assert!(plan.segments[0].embedded);
    assert_eq!(plan.segments[1].start, 0x0410);
    assert_eq!(plan.segments[2].start, 0x0C10);
}

#[test]
fn memory_loads_sort_address_ascending_regardless_of_toml_order() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "ordering"
        [targets.cor24]
        kind = "emulator"
        word_bits = 24
        address_bits = 24
        endian = "big"
        loader = "cor24-memory-map"
        regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
        [scenarios.demo]
        target = "cor24"
        layers = ["high", "low"]
        entry  = "0x000000"
        [scenarios.demo.run]
        max_cycles = 1
        [layers.high]
        kind = "binary"
        input = "high.bin"
        [layers.high.load]
        method = "memory"
        address = "0x010000"
        [layers.low]
        kind = "binary"
        input = "low.bin"
        [layers.low.load]
        method = "memory"
        address = "0x000000"
    "#;
    let cfg = parse(toml);
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default()).unwrap();
    let addrs: Vec<u32> = plan.memory_loads.iter().map(|m| m.address).collect();
    assert_eq!(addrs, vec![0x000000, 0x010000]);
}

#[test]
fn cross_layer_symbol_resolves_through_listing() {
    // Layer "vm" loads at 0x000000 and exports `code_ptr` at
    // offset 0x0010. Layer "app" patches "vm.code_ptr" with a
    // literal hex value. Phase 2 step 2 resolves the target to
    // 0x0010 (vm.address + offset).
    let toml = r#"
schema_version = 1
[project]
name = "sym-resolve"
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
input = "vm.s"
exports = { symbols = ["code_ptr"] }
[layers.vm.load]
method = "memory"
address = "0x000000"
[layers.app]
kind = "binary"
input = "app.bin"
patches = [
  { target = "vm.code_ptr", value = "0x010000" },
]
[layers.app.load]
method = "memory"
address = "0x010000"
"#;
    let cfg = parse(toml);
    let mut by_layer = BTreeMap::new();
    let lst = "                    _start:\n\
               0000: 00 00 00       .word 0\n\
                                    code_ptr:\n\
               0010: 00 00 00       .word 0\n";
    by_layer.insert(
        "vm".to_string(),
        ArtifactEntry {
            artifact: Utf8PathBuf::from("vm.bin"),
            listing: Listing::parse(lst),
        },
    );
    by_layer.insert(
        "app".to_string(),
        ArtifactEntry {
            artifact: Utf8PathBuf::from("app.bin"),
            listing: Listing::default(),
        },
    );
    let arts = Artifacts { by_layer };
    let plan = LoadPlan::build(&cfg, "demo", &arts).unwrap();
    assert_eq!(plan.patches.len(), 1);
    assert_eq!(plan.patches[0].address, 0x0010);
    assert_eq!(plan.patches[0].value, 0x010000);
}

#[test]
fn cross_layer_value_address_resolves_to_layer_load_addr() {
    let toml = r#"
schema_version = 1
[project]
name = "addr-form"
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
input = "vm.s"
exports = { symbols = ["code_ptr"] }
[layers.vm.load]
method = "memory"
address = "0x000000"
[layers.app]
kind = "binary"
input = "app.bin"
patches = [
  { target = "vm.code_ptr", value = "app.address" },
]
[layers.app.load]
method = "memory"
address = "0x010000"
"#;
    let cfg = parse(toml);
    let mut by_layer = BTreeMap::new();
    let lst = "                    code_ptr:\n0010: 00 00 00       .word 0\n";
    by_layer.insert(
        "vm".to_string(),
        ArtifactEntry {
            artifact: Utf8PathBuf::from("vm.bin"),
            listing: Listing::parse(lst),
        },
    );
    by_layer.insert(
        "app".to_string(),
        ArtifactEntry {
            artifact: Utf8PathBuf::from("app.bin"),
            listing: Listing::default(),
        },
    );
    let arts = Artifacts { by_layer };
    let plan = LoadPlan::build(&cfg, "demo", &arts).unwrap();
    assert_eq!(plan.patches[0].address, 0x0010);
    assert_eq!(plan.patches[0].value, 0x010000);
}

#[test]
fn literal_hex_patches_resolve_in_address_order() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "patches"
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
        [scenarios.demo.run]
        max_cycles = 1
        [layers.a]
        kind = "binary"
        input = "a.bin"
        patches = [
          { target = "0x000B22", value = "0x000099" },
          { target = "0x000A11", value = "0x000042" },
        ]
        [layers.a.load]
        method = "memory"
        address = "0x000000"
    "#;
    let cfg = parse(toml);
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default()).unwrap();
    assert_eq!(plan.patches.len(), 2);
    assert_eq!(plan.patches[0].address, 0x000A11);
    assert_eq!(plan.patches[1].address, 0x000B22);
}
