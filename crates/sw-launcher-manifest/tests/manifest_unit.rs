//! Snapshot tests for `manifest::LoadPlan` and the cor24 argv it
//! emits. The snapshots are inline strings (no external snapshot
//! crate) so a churn in formatting fails loudly here rather than
//! later.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use sw_launcher_config::Config;
use sw_launcher_manifest::{ArtifactEntry, Artifacts, LoadPlan};
use sw_launcher_tool::tool::Listing;

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
    p.push("../../tests/fixtures");
    p
}

#[test]
fn scenario_a_load_plan_has_expected_shape() {
    let dir = fixtures_dir();
    let cfg = parse(&scenario_a_toml(&dir));
    let arts = artifacts_for_echo(&dir);
    let plan = LoadPlan::build(&cfg, "echo", &arts, Utf8Path::new(".")).expect("plan builds");

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
    let plan = LoadPlan::build(&cfg, "echo", &arts, Utf8Path::new(".")).unwrap();
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
    let plan = LoadPlan::build(&cfg, "echo", &arts, Utf8Path::new(".")).unwrap();

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
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), Utf8Path::new(".")).unwrap();
    let addrs: Vec<u32> = plan.memory_loads.iter().map(|m| m.address).collect();
    assert_eq!(addrs, vec![0x000000, 0x010000]);
}

fn sidecar_toml(value_term: &str) -> String {
    format!(
        r#"
schema_version = 1
[project]
name = "sidecar-test"
[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"
regions = {{ sram = {{ start = "0x000000", end = "0x0FFFFF" }}, ebr_stack = {{ start = "0xFEEC00", end = "0xFEF7FF" }}, mmio = {{ start = "0xFF0000", end = "0xFFFFFF" }} }}
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
  {{ target = "0x000A12", value = "{value_term}" }},
]
[layers.a.load]
method = "memory"
address = "0x000000"
"#
    )
}

fn write_sidecar(dir: &Utf8Path, name: &str, contents: &str) -> Utf8PathBuf {
    let p = dir.join(name);
    std::fs::write(p.as_std_path(), contents).unwrap();
    p
}

fn tempdir_utf8() -> Utf8PathBuf {
    let base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!(
        "sw-launcher-sidecar-{}-{nanos:x}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Utf8PathBuf::from_path_buf(dir).unwrap()
}

#[test]
fn sidecar_resolves_to_literal_hex_value() {
    let dir = tempdir_utf8();
    write_sidecar(&dir, "addr.txt", "0x001279\n");
    let cfg = parse(&sidecar_toml("sidecar:addr.txt"));
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), &dir).unwrap();
    assert_eq!(plan.patches.len(), 1);
    assert_eq!(plan.patches[0].address, 0x000A12);
    assert_eq!(plan.patches[0].value, 0x001279);

    // Also accept bare hex (no 0x prefix), trimmed whitespace.
    write_sidecar(&dir, "addr.txt", "  1279  \n");
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), &dir).unwrap();
    assert_eq!(plan.patches[0].value, 0x001279);
}

#[test]
fn missing_sidecar_produces_clear_error() {
    let dir = tempdir_utf8();
    let cfg = parse(&sidecar_toml("sidecar:nonexistent.txt"));
    let err = LoadPlan::build(&cfg, "demo", &Artifacts::default(), &dir).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("E0019"), "got: {msg}");
    assert!(msg.contains("nonexistent.txt"), "got: {msg}");
}

#[test]
fn sidecar_with_non_hex_contents_errors() {
    let dir = tempdir_utf8();
    write_sidecar(&dir, "garbage.txt", "not a number\n");
    let cfg = parse(&sidecar_toml("sidecar:garbage.txt"));
    let err = LoadPlan::build(&cfg, "demo", &Artifacts::default(), &dir).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("E0019"), "got: {msg}");
    assert!(
        msg.contains("hex literal") || msg.contains("not a number"),
        "got: {msg}"
    );
    assert!(msg.contains("garbage.txt"), "got: {msg}");
}

fn uart_two_chunk_toml(input_a: &Utf8Path, input_b: &Utf8Path) -> String {
    format!(
        r#"
schema_version = 1
[project]
name = "uart-multi"
[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"
regions = {{ sram = {{ start = "0x000000", end = "0x0FFFFF" }}, ebr_stack = {{ start = "0xFEEC00", end = "0xFEF7FF" }}, mmio = {{ start = "0xFF0000", end = "0xFFFFFF" }} }}
[scenarios.demo]
target = "cor24"
layers = ["src", "stdin"]
entry  = "0x000000"
[scenarios.demo.run]
max_cycles = 1
[layers.src]
kind = "text"
input = "{a}"
[layers.src.load]
method = "uart"
max_bytes = 16384
terminator = "EOT"
[layers.stdin]
kind = "text"
input = "{b}"
[layers.stdin.load]
method = "uart"
max_bytes = 16384
terminator = "none"
"#,
        a = input_a,
        b = input_b
    )
}

#[test]
fn two_uart_chunks_in_declared_order_concatenate_with_terminators() {
    let dir = tempdir_utf8();
    let a = write_sidecar(&dir, "a.ml", "let x = 1 + 2;\n");
    let b = write_sidecar(&dir, "b.txt", "exit 0\n");
    let cfg = parse(&uart_two_chunk_toml(&a, &b));
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), &dir).unwrap();
    let argv = plan.cor24_argv();
    let payload_idx = argv.iter().position(|s| s == "--uart-input").unwrap() + 1;
    // src bytes + EOT (0x04) + stdin bytes (no terminator).
    let expected = "let x = 1 + 2;\n\u{04}exit 0\n";
    assert_eq!(argv[payload_idx], expected);
}

#[test]
fn terminator_none_appends_no_byte() {
    let dir = tempdir_utf8();
    let a = write_sidecar(&dir, "x.txt", "x");
    let toml = format!(
        r#"
schema_version = 1
[project]
name = "uart-none"
[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"
regions = {{ sram = {{ start = "0x000000", end = "0x0FFFFF" }}, ebr_stack = {{ start = "0xFEEC00", end = "0xFEF7FF" }}, mmio = {{ start = "0xFF0000", end = "0xFFFFFF" }} }}
[scenarios.demo]
target = "cor24"
layers = ["one"]
entry  = "0x000000"
[scenarios.demo.run]
max_cycles = 1
[layers.one]
kind = "text"
input = "{a}"
[layers.one.load]
method = "uart"
max_bytes = 1024
terminator = "none"
"#,
        a = a
    );
    let cfg = parse(&toml);
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), &dir).unwrap();
    let argv = plan.cor24_argv();
    let payload_idx = argv.iter().position(|s| s == "--uart-input").unwrap() + 1;
    assert_eq!(argv[payload_idx], "x");
}

#[test]
fn empty_input_file_just_terminator() {
    let dir = tempdir_utf8();
    let a = write_sidecar(&dir, "empty.txt", "");
    let toml = format!(
        r#"
schema_version = 1
[project]
name = "uart-empty"
[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"
regions = {{ sram = {{ start = "0x000000", end = "0x0FFFFF" }}, ebr_stack = {{ start = "0xFEEC00", end = "0xFEF7FF" }}, mmio = {{ start = "0xFF0000", end = "0xFFFFFF" }} }}
[scenarios.demo]
target = "cor24"
layers = ["one"]
entry  = "0x000000"
[scenarios.demo.run]
max_cycles = 1
[layers.one]
kind = "text"
input = "{a}"
[layers.one.load]
method = "uart"
max_bytes = 1024
terminator = "EOT"
"#,
        a = a
    );
    let cfg = parse(&toml);
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), &dir).unwrap();
    let argv = plan.cor24_argv();
    let payload_idx = argv.iter().position(|s| s == "--uart-input").unwrap() + 1;
    assert_eq!(argv[payload_idx], "\u{04}");
}

#[test]
fn reserved_heap_segment_appears_in_segments_not_loads() {
    let toml = r#"
schema_version = 1
[project]
name = "reserved-heap"
[targets.cor24]
kind = "emulator"
word_bits = 24
address_bits = 24
endian = "big"
loader = "cor24-memory-map"
regions = { sram = { start = "0x000000", end = "0x0FFFFF" }, ebr_stack = { start = "0xFEEC00", end = "0xFEF7FF" }, mmio = { start = "0xFF0000", end = "0xFFFFFF" } }
[scenarios.demo]
target = "cor24"
layers = ["app"]
entry  = "0x000000"
[scenarios.demo.run]
max_cycles = 1
[layers.app]
kind = "binary"
input = "app.bin"
[layers.app.load]
method = "memory"
address = "0x000000"
[[layers.app.segments]]
name = "value_heap"
kind = "heap"
embedded = false
size = "0x010000"
[layers.app.segments.load]
method = "memory"
address = "0x080000"
"#;
    let cfg = parse(toml);
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), Utf8Path::new(".")).unwrap();
    // memory_loads has only the binary layer; the reserved heap
    // segment does NOT contribute a --load-binary entry.
    assert_eq!(plan.memory_loads.len(), 1);
    assert_eq!(plan.memory_loads[0].layer, "app");
    assert_eq!(plan.memory_loads[0].address, 0x000000);
    // segments has the heap reservation, with the right range.
    assert_eq!(plan.segments.len(), 1);
    assert_eq!(plan.segments[0].name.as_deref(), Some("value_heap"));
    assert_eq!(plan.segments[0].start, 0x080000);
    assert_eq!(plan.segments[0].end, 0x080000 + 0x010000);
    assert!(!plan.segments[0].embedded);
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
    let plan = LoadPlan::build(&cfg, "demo", &arts, Utf8Path::new(".")).unwrap();
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
    let plan = LoadPlan::build(&cfg, "demo", &arts, Utf8Path::new(".")).unwrap();
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
    let plan = LoadPlan::build(&cfg, "demo", &Artifacts::default(), Utf8Path::new(".")).unwrap();
    assert_eq!(plan.patches.len(), 2);
    assert_eq!(plan.patches[0].address, 0x000A11);
    assert_eq!(plan.patches[1].address, 0x000B22);
}
