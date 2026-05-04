//! Typed mirror of `sw-launch.toml`, schema v1.2.
//!
//! This module is *parsing only*. Cross-field validation
//! (overlap, kind/load compatibility, profile resolution, heap
//! budgets) lives in `validate.rs` (step 006).
//!
//! Every struct uses `serde(deny_unknown_fields)` to catch typos
//! at deserialize time. Hex-shaped strings (addresses, sizes) are
//! preserved as `HexValue` and parsed on demand via
//! `HexValue::as_u32()`.
//!
//! See `docs/design.md` for the schema and `docs/heap-analysis.md`
//! for the budget rationale.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;

use anyhow::{Context, Result, anyhow};
use camino::Utf8Path;
use serde::Deserialize;

/// Stable identifier for a user-visible diagnostic.
///
/// Format: `E0xxx` where `xxx` is a zero-padded decimal. Lives in
/// `sw-launcher-config` (since both the validate sub-crate and
/// the main crate's `error` module need it). The full catalogue
/// is in `docs/design.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCode(pub u16);

impl ErrorCode {
    /// `E0090`: action requested is implemented in a later step.
    pub const NOT_IMPLEMENTED: ErrorCode = ErrorCode(90);
    /// `E0091`: malformed CLI arguments not caught by clap itself.
    pub const CLI: ErrorCode = ErrorCode(91);
    /// `E0092`: I/O error reading or writing a file.
    pub const IO: ErrorCode = ErrorCode(92);
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{:04}", self.0)
    }
}

/// Top-level shape of `sw-launch.toml`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Schema version; v1.2 still uses the integer `1` (the minor
    /// bump is tracked in docs/status.md, not in TOML).
    pub schema_version: u32,
    pub project: Project,
    #[serde(default)]
    pub targets: BTreeMap<String, Target>,
    #[serde(default)]
    pub memory_profiles: BTreeMap<String, MemoryProfile>,
    #[serde(default)]
    pub scenarios: BTreeMap<String, Scenario>,
    #[serde(default)]
    pub layers: BTreeMap<String, Layer>,
}

impl Config {
    /// Parse a `Config` from a TOML string.
    ///
    /// Named `from_toml_str` (not `from_str`) so we don't shadow
    /// the `std::str::FromStr` trait method's signature, which
    /// expects a `Result<Self, Self::Err>` shape we don't want to
    /// commit to as a public contract.
    pub fn from_toml_str(s: &str) -> Result<Self> {
        let cfg: Config = toml::from_str(s).map_err(|e| anyhow!("{e}"))?;
        if cfg.schema_version != 1 {
            return Err(anyhow!(
                "schema_version must be 1 (got {})",
                cfg.schema_version
            ));
        }
        Ok(cfg)
    }

    /// Parse a `Config` from a path on disk.
    pub fn from_path(path: &Utf8Path) -> Result<Self> {
        let s = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        Self::from_toml_str(&s)
    }
}

/// `[project]` block.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub default_target: Option<String>,
    #[serde(default)]
    pub default_scenario: Option<String>,
    #[serde(default)]
    pub root: Option<String>,
}

/// `[targets.<name>]` block.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub kind: String,
    pub word_bits: u32,
    pub address_bits: u32,
    pub endian: String,
    pub loader: String,
    pub regions: Regions,
    #[serde(default)]
    pub run_defaults: Option<RunDefaults>,
}

/// `[targets.<name>.regions]` block.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Regions {
    pub sram: MemRange,
    pub ebr_stack: MemRange,
    pub mmio: MemRange,
}

/// A `{ start, end, role? }` triple in TOML.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MemRange {
    pub start: HexValue,
    pub end: HexValue,
    #[serde(default)]
    pub role: Option<String>,
}

/// `[targets.<n>.run_defaults]`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunDefaults {
    #[serde(default)]
    pub stack_kilobytes: Option<u32>,
}

/// `[memory_profiles.<name>]` block.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MemoryProfile {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub partitions: Vec<Partition>,
    #[serde(default)]
    pub budget: Option<Budget>,
}

/// One partition in a `[memory_profiles.<name>]`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Partition {
    pub name: String,
    pub base: HexValue,
    pub size: HexValue,
    #[serde(default)]
    pub regions: Vec<ProfileRegion>,
}

/// One named region inside a partition.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProfileRegion {
    pub name: String,
    pub kind: MemRegionKind,
    pub size: SizeOrAuto,
}

/// `kind` enum for profile regions and segments.
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum MemRegionKind {
    Code,
    Data,
    Bss,
    Heap,
    Stack,
    Mmio,
    Spare,
}

/// `[memory_profiles.<name>.budget]`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub code_max: HexValue,
    pub heap_max: HexValue,
    pub stack_max: HexValue,
    pub total_max: HexValue,
    #[serde(default = "default_true")]
    pub justification_required: bool,
}

fn default_true() -> bool {
    true
}

/// `[scenarios.<name>]` block.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub target: String,
    #[serde(default)]
    pub memory_profile: Option<String>,
    #[serde(default)]
    pub layers: Vec<String>,
    pub entry: HexValue,
    #[serde(default)]
    pub run: Option<RunCfg>,
    #[serde(default)]
    pub expect: Option<Expect>,
}

/// `[scenarios.<n>.run]`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunCfg {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub max_cycles: Option<u64>,
    #[serde(default)]
    pub halt_on: Option<String>,
    #[serde(default)]
    pub stack_kilobytes: Option<u32>,
}

/// `[scenarios.<n>.expect]`.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    pub uart_contains: Vec<String>,
    #[serde(default)]
    pub uart_regex: Vec<String>,
    #[serde(default)]
    pub uart_not_contains: Vec<String>,
    #[serde(default)]
    pub stdout_lines_eq: Vec<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

/// `[layers.<name>]` block.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub kind: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub artifact: Option<String>,
    #[serde(default)]
    pub size: Option<SizeOrAuto>,
    #[serde(default)]
    pub absolute_addresses: bool,
    #[serde(default)]
    pub acknowledge_oversized: bool,
    #[serde(default)]
    pub load: Option<LoadSpec>,
    #[serde(default)]
    pub exports: Option<Exports>,
    #[serde(default)]
    pub patches: Vec<Patch>,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub heap_justification: Option<HeapJustification>,
}

/// `load.method = "memory" | "uart"` plus its parameters.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoadSpec {
    pub method: LoadMethod,
    #[serde(default)]
    pub address: Option<HexValue>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub terminator: Option<String>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub in_band_terminator: Option<String>,
}

/// Load method enum.
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum LoadMethod {
    Memory,
    Uart,
}

/// `exports = { symbols = [...] }`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Exports {
    #[serde(default)]
    pub symbols: Vec<String>,
}

/// `patches = [{ target, value }, ...]`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub target: String,
    pub value: String,
}

/// `[[layers.<n>.segments]]`.
///
/// v1.2: a segment can either declare absolute `(load.address,
/// size)`, claim `(partition, region)` from the scenario's profile,
/// or be embedded in the artifact at a named `symbol`. Validation
/// (step 006) enforces that exactly one form is provided per
/// segment kind.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    #[serde(default)]
    pub name: Option<String>,
    pub kind: SegmentKind,
    #[serde(default)]
    pub embedded: Option<bool>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub size: Option<SizeOrAuto>,
    #[serde(default)]
    pub load: Option<LoadSpec>,
    #[serde(default)]
    pub claims: Vec<SegmentClaim>,
    #[serde(default)]
    pub grows: Option<String>,
    #[serde(default)]
    pub patches: Vec<Patch>,
}

/// Segment kind.
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SegmentKind {
    Code,
    Data,
    Bss,
    Heap,
    Stack,
    Mmio,
}

/// One element of `segments[<n>].claims`.
///
/// `partition` and `region` are profile-relative names. A claim
/// may also be expressed as `{ layer = "...", segment = "...",
/// role = "after" | "before" }` for adjacency-encoded heaps.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SegmentClaim {
    #[serde(default)]
    pub partition: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub segment: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

/// `[layers.<n>.heap_justification]`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeapJustification {
    pub category: HeapCategory,
    pub note: String,
    #[serde(default)]
    pub measured_floor_kib: Option<u32>,
    #[serde(default)]
    pub tracking_issue: Option<String>,
}

/// `heap_justification.category` enum from `docs/memory-stance.md`.
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum HeapCategory {
    AlgorithmicFloor,
    BytecodeImage,
    GcSlack,
    DeadLeak,
    AlgorithmicBloat,
}

/// A hex-formatted u32 address or size.
///
/// The TOML form is `"0x010000"` or `"0x01_0000"`. Stored as a
/// `String` so deserialization is total; conversion to `u32` via
/// `as_u32()` reports a precise error on bad input.
#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
#[serde(transparent)]
pub struct HexValue(pub String);

impl HexValue {
    /// Parse the underlying string as a u32 hex literal.
    pub fn as_u32(&self) -> Result<u32> {
        parse_hex_u32(&self.0)
    }
}

/// Either a `HexValue` or the literal string `"auto"`. Used for
/// segment / region sizes that can defer to the artifact length.
#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum SizeOrAuto {
    /// Actual hex size.
    Hex(HexValue),
}

impl SizeOrAuto {
    /// Returns the hex value if this is not `"auto"`.
    pub fn as_hex(&self) -> Option<&HexValue> {
        match self {
            SizeOrAuto::Hex(h) if h.0 != "auto" => Some(h),
            _ => None,
        }
    }

    /// Returns true if this slot is the literal `"auto"`.
    pub fn is_auto(&self) -> bool {
        matches!(self, SizeOrAuto::Hex(h) if h.0 == "auto")
    }
}

/// Parse a string of the form `0x[0-9a-fA-F_]+` as a u32.
pub fn parse_hex_u32(s: &str) -> Result<u32> {
    let stripped = s
        .strip_prefix("0x")
        .ok_or_else(|| anyhow!("hex literal must start with 0x: {s:?}"))?;
    let cleaned: String = stripped.chars().filter(|&c| c != '_').collect();
    if cleaned.is_empty() {
        return Err(anyhow!("hex literal has no digits: {s:?}"));
    }
    u32::from_str_radix(&cleaned, 16).map_err(|e| anyhow!("invalid hex literal {s:?}: {e}"))
}

// Unit tests live in `tests/config_unit.rs` (integration test).
// The tests target only `pub` items from this module, which keeps
// the source file under the per-module size and function-count
// budgets enforced by sw-checklist.
