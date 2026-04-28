//! Translate a validated `Config` + scenario + built artifacts
//! into a deterministic `LoadPlan`, then into the `cor24-run`
//! command line. Step 008 covers Scenario A; later steps add B/C
//! patches and composite-image emission.
//!
//! The plan is *deterministic*: memory loads come out address-
//! ascending regardless of TOML declaration order, patches come
//! out address-ascending too. UART chunks preserve the scenario's
//! declared order (the order matters for the `<source> + EOT +
//! <stdin>` pattern from `docs/survey/ocaml.md`).
//!
//! Embedded segments (e.g. `pvm.s`'s `eval_stack:`) contribute to
//! `LoadPlan.segments` for overlap checks but never produce
//! `--load-binary` arguments.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;

use crate::config::{Config, LoadMethod, Scenario, SegmentKind, SizeOrAuto};
use crate::error::{Error, Result};
use crate::tool::Listing;

/// Map of `layer name -> (artifact path, listing)`. Filled by the
/// step-007 `Assembler` and consumed here. For pure-data layers
/// the listing is empty; the artifact path is required.
#[derive(Debug, Default, Clone)]
pub struct Artifacts {
    pub by_layer: BTreeMap<String, ArtifactEntry>,
}

#[derive(Debug, Clone)]
pub struct ArtifactEntry {
    pub artifact: Utf8PathBuf,
    pub listing: Listing,
}

/// What to load, where, and what to patch before launching the
/// emulator.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadPlan {
    pub memory_loads: Vec<MemoryLoad>,
    pub uart: Vec<UartChunk>,
    pub patches: Vec<ResolvedPatch>,
    pub entry: u32,
    /// Every segment claimed by every layer in the scenario, in
    /// declaration order. Embedded segments are included but
    /// contribute no MemoryLoad.
    pub segments: Vec<ResolvedSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryLoad {
    pub layer: String,
    pub path: Utf8PathBuf,
    pub address: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UartChunk {
    pub layer: String,
    pub bytes: Vec<u8>,
    pub terminator: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPatch {
    pub address: u32,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSegment {
    pub layer: String,
    pub name: Option<String>,
    pub kind: SegmentKind,
    pub start: u32,
    pub end: u32,
    pub embedded: bool,
}

impl LoadPlan {
    /// Construct a `LoadPlan` from a config + scenario name +
    /// already-built artifacts. Returns `Err` only on shape
    /// problems that validation should have caught earlier
    /// (missing scenario, missing artifact for a memory layer,
    /// unparseable hex).
    pub fn build(cfg: &Config, scenario_name: &str, artifacts: &Artifacts) -> Result<LoadPlan> {
        let scen = cfg
            .scenarios
            .get(scenario_name)
            .ok_or_else(|| Error::cli(format!("scenario `{scenario_name}` not declared")))?;
        let mut plan = LoadPlan {
            entry: scen.entry.as_u32()?,
            ..LoadPlan::default()
        };
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            collect_layer(layer_name, layer, artifacts, scen, &mut plan)?;
        }
        plan.memory_loads.sort_by_key(|m| m.address);
        plan.patches.sort_by_key(|p| p.address);
        Ok(plan)
    }

    /// Translate this plan into the argv `cor24-run` expects,
    /// without spawning anything. Used by step 009 (runner) and
    /// by the snapshot tests below.
    pub fn cor24_argv(&self) -> Vec<String> {
        let mut argv: Vec<String> = Vec::new();
        for m in &self.memory_loads {
            argv.push("--load-binary".into());
            argv.push(format!(
                "{path}@0x{addr:06X}",
                path = m.path,
                addr = m.address
            ));
        }
        for p in &self.patches {
            argv.push("--patch".into());
            argv.push(format!(
                "0x{addr:06X}=0x{value:06X}",
                addr = p.address,
                value = p.value
            ));
        }
        argv.push("--entry".into());
        argv.push(format!("0x{:06X}", self.entry));
        if !self.uart.is_empty() {
            let mut payload: Vec<u8> = Vec::new();
            for chunk in &self.uart {
                payload.extend_from_slice(&chunk.bytes);
                if let Some(t) = chunk.terminator {
                    payload.push(t);
                }
            }
            argv.push("--uart-input".into());
            argv.push(String::from_utf8_lossy(&payload).into_owned());
        }
        argv
    }
}

fn collect_layer(
    layer_name: &str,
    layer: &crate::config::Layer,
    artifacts: &Artifacts,
    scen: &Scenario,
    plan: &mut LoadPlan,
) -> Result<()> {
    if let Some(load) = &layer.load {
        match load.method {
            LoadMethod::Memory => collect_memory(layer_name, layer, load, artifacts, plan)?,
            LoadMethod::Uart => collect_uart(layer_name, layer, load, plan)?,
        }
    }
    let _ = scen; // used by step 009 once profile-relative claims resolve
    for (idx, seg) in layer.segments.iter().enumerate() {
        if let Some(rs) = resolve_segment(layer_name, idx, seg, layer, artifacts) {
            plan.segments.push(rs);
        }
    }
    for p in &layer.patches {
        if let Some(rp) = resolve_patch(p, layer_name) {
            plan.patches.push(rp);
        }
    }
    Ok(())
}

fn collect_memory(
    layer_name: &str,
    layer: &crate::config::Layer,
    load: &crate::config::LoadSpec,
    artifacts: &Artifacts,
    plan: &mut LoadPlan,
) -> Result<()> {
    let Some(addr_hex) = &load.address else {
        return Ok(());
    };
    let addr = addr_hex.as_u32()?;
    let path = artifacts
        .by_layer
        .get(layer_name)
        .map(|a| a.artifact.clone())
        .or_else(|| layer.input.as_deref().map(Utf8PathBuf::from))
        .ok_or_else(|| Error::cli(format!("layer `{layer_name}` has no built artifact path")))?;
    plan.memory_loads.push(MemoryLoad {
        layer: layer_name.into(),
        path,
        address: addr,
    });
    Ok(())
}

fn collect_uart(
    layer_name: &str,
    layer: &crate::config::Layer,
    load: &crate::config::LoadSpec,
    plan: &mut LoadPlan,
) -> Result<()> {
    let bytes = if let Some(input) = &layer.input {
        std::fs::read(input).map_err(|source| Error::Io { source })?
    } else {
        Vec::new()
    };
    let terminator = load.terminator.as_deref().and_then(|n| match n {
        "EOT" => Some(0x04u8),
        "ETX" => Some(0x03u8),
        "EOF" => Some(0x1Au8),
        _ => None,
    });
    plan.uart.push(UartChunk {
        layer: layer_name.into(),
        bytes,
        terminator,
    });
    Ok(())
}

fn resolve_segment(
    layer_name: &str,
    idx: usize,
    seg: &crate::config::Segment,
    layer: &crate::config::Layer,
    artifacts: &Artifacts,
) -> Option<ResolvedSegment> {
    let size = seg
        .size
        .as_ref()
        .and_then(SizeOrAuto::as_hex)
        .and_then(|h| h.as_u32().ok())
        .unwrap_or(0);
    let start = if seg.embedded == Some(true) {
        let symbol = seg.symbol.as_deref()?;
        artifacts
            .by_layer
            .get(layer_name)?
            .listing
            .resolve(symbol)?
    } else {
        seg.load.as_ref()?.address.as_ref()?.as_u32().ok()?
    };
    let _ = (idx, layer); // placeholders for step 009 partition-claim resolution
    Some(ResolvedSegment {
        layer: layer_name.into(),
        name: seg.name.clone(),
        kind: seg.kind,
        start,
        end: start.saturating_add(size),
        embedded: seg.embedded == Some(true),
    })
}

fn resolve_patch(p: &crate::config::Patch, _layer_name: &str) -> Option<ResolvedPatch> {
    if !p.target.starts_with("0x") {
        return None; // symbolic targets resolve in step 009
    }
    let address = crate::config::parse_hex_u32(&p.target).ok()?;
    let value = if p.value.starts_with("0x") {
        crate::config::parse_hex_u32(&p.value).ok()?
    } else {
        return None;
    };
    Some(ResolvedPatch { address, value })
}
