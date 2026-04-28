//! Unit tests for `sw_launcher::config`.
//!
//! Lives outside `src/config.rs` so the source module stays under
//! sw-checklist's file-LOC and function-count budgets.

use sw_launcher::config::{
    Config, HeapCategory, HexValue, LoadMethod, MemRegionKind, SegmentKind, SizeOrAuto,
    parse_hex_u32,
};

#[test]
fn parse_hex_accepts_underscores_and_case() {
    assert_eq!(parse_hex_u32("0x000000").unwrap(), 0x000000);
    assert_eq!(parse_hex_u32("0x010000").unwrap(), 0x010000);
    assert_eq!(parse_hex_u32("0x01_00_00").unwrap(), 0x010000);
    assert_eq!(parse_hex_u32("0xFEEC00").unwrap(), 0xFEEC00);
    assert_eq!(parse_hex_u32("0xfeec00").unwrap(), 0xFEEC00);
}

#[test]
fn parse_hex_rejects_missing_prefix_or_digits() {
    assert!(parse_hex_u32("010000").is_err());
    assert!(parse_hex_u32("0x").is_err());
    assert!(parse_hex_u32("0xZZ").is_err());
}

#[test]
fn schema_version_must_be_one() {
    let toml = r#"
        schema_version = 2
        [project]
        name = "x"
    "#;
    let err = Config::from_toml_str(toml).unwrap_err();
    assert!(err.to_string().contains("schema_version"));
}

#[test]
fn unknown_top_level_key_rejects() {
    let toml = r#"
        schema_version = 1
        mystery = true
        [project]
        name = "x"
    "#;
    let err = Config::from_toml_str(toml).unwrap_err();
    assert!(err.to_string().contains("unknown") || err.to_string().contains("mystery"));
}

#[test]
fn unknown_layer_key_rejects() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "x"
        [layers.foo]
        kind = "binary"
        mystery_field = 42
    "#;
    let err = Config::from_toml_str(toml).unwrap_err();
    assert!(err.to_string().contains("unknown") || err.to_string().contains("mystery"));
}

#[test]
fn segment_embedded_with_symbol_parses() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "x"
        [layers.pvm]
        kind = "assembler"
        [[layers.pvm.segments]]
        name = "eval_stack"
        kind = "stack"
        embedded = true
        symbol = "eval_stack"
        size = "0x000400"
    "#;
    let cfg = Config::from_toml_str(toml).unwrap();
    let pvm = cfg.layers.get("pvm").unwrap();
    assert_eq!(pvm.segments.len(), 1);
    let seg = &pvm.segments[0];
    assert_eq!(seg.kind, SegmentKind::Stack);
    assert_eq!(seg.embedded, Some(true));
    assert_eq!(seg.symbol.as_deref(), Some("eval_stack"));
}

#[test]
fn segment_reserved_with_load_address_parses() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "x"
        [layers.heap]
        kind = "data"
        [[layers.heap.segments]]
        name = "ocaml_heap"
        kind = "heap"
        embedded = false
        size = "0x040000"
        grows = "down"
        [layers.heap.segments.load]
        method = "memory"
        address = "0x080000"
    "#;
    let cfg = Config::from_toml_str(toml).unwrap();
    let layer = cfg.layers.get("heap").unwrap();
    let seg = &layer.segments[0];
    assert_eq!(seg.embedded, Some(false));
    assert_eq!(seg.grows.as_deref(), Some("down"));
    let load = seg.load.as_ref().unwrap();
    assert_eq!(load.method, LoadMethod::Memory);
    assert_eq!(load.address.as_ref().unwrap().as_u32().unwrap(), 0x080000);
}

#[test]
fn patch_self_address_value_parses_as_string() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "x"
        [layers.foo]
        kind = "binary"
        patches = [
          { target = "foo.heap_limit", value = "self.end" },
          { target = "0x000A12",       value = "0x000042" },
        ]
    "#;
    let cfg = Config::from_toml_str(toml).unwrap();
    let foo = cfg.layers.get("foo").unwrap();
    assert_eq!(foo.patches.len(), 2);
    assert_eq!(foo.patches[0].value, "self.end");
    assert_eq!(foo.patches[1].value, "0x000042");
}

#[test]
fn region_kind_enum_parses_lowercase() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "x"
        [memory_profiles.compiled-app]
        [[memory_profiles.compiled-app.partitions]]
        name = "p"
        base = "0x000000"
        size = "0x020000"
        regions = [
          { name = "c", kind = "code",  size = "auto"     },
          { name = "h", kind = "heap",  size = "0x008000" },
          { name = "s", kind = "stack", size = "0x002000" },
        ]
    "#;
    let cfg = Config::from_toml_str(toml).unwrap();
    let parts = &cfg.memory_profiles["compiled-app"].partitions;
    assert_eq!(parts[0].regions[0].kind, MemRegionKind::Code);
    assert_eq!(parts[0].regions[1].kind, MemRegionKind::Heap);
    assert_eq!(parts[0].regions[2].kind, MemRegionKind::Stack);
}

#[test]
fn heap_justification_categories_parse_kebab_case() {
    let toml = r#"
        schema_version = 1
        [project]
        name = "x"
        [layers.foo]
        kind = "binary"
        heap_justification = { category = "gc-slack", note = "ok", measured_floor_kib = 64 }
    "#;
    let cfg = Config::from_toml_str(toml).unwrap();
    let j = cfg.layers["foo"].heap_justification.as_ref().unwrap();
    assert_eq!(j.category, HeapCategory::GcSlack);
    assert_eq!(j.measured_floor_kib, Some(64));
}

#[test]
fn size_or_auto_distinguishes_auto_and_hex() {
    let auto = SizeOrAuto::Hex(HexValue("auto".into()));
    let hex = SizeOrAuto::Hex(HexValue("0x010000".into()));
    assert!(auto.is_auto());
    assert!(!hex.is_auto());
    assert!(auto.as_hex().is_none());
    assert_eq!(hex.as_hex().unwrap().as_u32().unwrap(), 0x010000);
}
