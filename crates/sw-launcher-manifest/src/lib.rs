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

use anyhow::{Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};

use sw_launcher_config::{Config, LoadMethod, Scenario, SegmentKind, SizeOrAuto};
use sw_launcher_tool::tool::Listing;

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
    pub fn build(
        cfg: &Config,
        scenario_name: &str,
        artifacts: &Artifacts,
        config_dir: &Utf8Path,
    ) -> Result<LoadPlan> {
        let scen = cfg
            .scenarios
            .get(scenario_name)
            .ok_or_else(|| anyhow!("scenario `{scenario_name}` not declared"))?;
        let mut plan = LoadPlan {
            entry: scen.entry.as_u32()?,
            ..LoadPlan::default()
        };
        // Pass 1: memory loads, UART, segments. No patches yet --
        // cross-layer patches need every layer's load.address known.
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            collect_layer(layer_name, layer, artifacts, scen, &mut plan)?;
        }
        plan.memory_loads.sort_by_key(|m| m.address);
        // Pass 2: resolve patches now that the plan knows where
        // every layer ended up. Literal hex stays inline; cross-
        // layer "<other>.<symbol>" looks up the producing layer's
        // listing in `artifacts`; "sidecar:<path>" reads a hex
        // value from a build-time text file (relative to
        // `config_dir`).
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            for p in &layer.patches {
                if let Some(rp) = resolve_patch(p, layer_name, cfg, artifacts, &plan, config_dir)? {
                    plan.patches.push(rp);
                }
            }
        }
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
    layer: &sw_launcher_config::Layer,
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
    Ok(())
}

fn collect_memory(
    layer_name: &str,
    layer: &sw_launcher_config::Layer,
    load: &sw_launcher_config::LoadSpec,
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
        .ok_or_else(|| anyhow!("layer `{layer_name}` has no built artifact path"))?;
    plan.memory_loads.push(MemoryLoad {
        layer: layer_name.into(),
        path,
        address: addr,
    });
    Ok(())
}

fn collect_uart(
    layer_name: &str,
    layer: &sw_launcher_config::Layer,
    load: &sw_launcher_config::LoadSpec,
    plan: &mut LoadPlan,
) -> Result<()> {
    let bytes = if let Some(input) = &layer.input {
        std::fs::read(input)?
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
    seg: &sw_launcher_config::Segment,
    layer: &sw_launcher_config::Layer,
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

/// Resolve a single `Patch` against the in-progress plan and the
/// built `Artifacts`. Targets and values can each be:
///
///   - literal hex (`"0x000A12"`)
///   - cross-layer symbol (`"<layer>.<symbol>"`) -- resolved
///     through the producing layer's `Listing` plus its load
///     address; the value form `"<layer>.address"` returns the
///     layer's load address verbatim.
///
/// Returns `Ok(None)` if the patch can't be resolved by *shape*
/// (e.g. a future `self.address` form); `Err` if the shape is
/// supported but resolution failed (missing listing, unknown
/// symbol).
fn resolve_patch(
    p: &sw_launcher_config::Patch,
    _layer_name: &str,
    cfg: &Config,
    artifacts: &Artifacts,
    plan: &LoadPlan,
    config_dir: &Utf8Path,
) -> Result<Option<ResolvedPatch>> {
    let address = match resolve_patch_term(&p.target, cfg, artifacts, plan, config_dir)? {
        Some(a) => a,
        None => return Ok(None),
    };
    let value = match resolve_patch_term(&p.value, cfg, artifacts, plan, config_dir)? {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(Some(ResolvedPatch { address, value }))
}

/// One of the two halves of a patch (target *or* value). Phase 3
/// shapes: hex-literal, `<layer>.<symbol-or-address>`, and
/// `sidecar:<path>` (path resolved relative to `config_dir` if
/// not absolute; file contents are a single hex value, with or
/// without `0x` prefix, whitespace tolerated).
fn resolve_patch_term(
    term: &str,
    cfg: &Config,
    artifacts: &Artifacts,
    plan: &LoadPlan,
    config_dir: &Utf8Path,
) -> Result<Option<u32>> {
    if term.starts_with("0x") {
        return Ok(Some(sw_launcher_config::parse_hex_u32(term)?));
    }
    if term == "self.address" || term == "self.end" || term == "self.size" {
        return Ok(None); // resolved by segment-block context, step 010
    }
    if let Some(rel) = term.strip_prefix("sidecar:") {
        let path = if Utf8Path::new(rel).is_absolute() {
            Utf8PathBuf::from(rel)
        } else {
            config_dir.join(rel)
        };
        let raw = std::fs::read_to_string(path.as_std_path()).map_err(|_| {
            anyhow!("E0019 sidecar `{path}` could not be read (referenced by patch term `{term}`)")
        })?;
        let cleaned = raw.trim();
        let stripped = cleaned.strip_prefix("0x").unwrap_or(cleaned);
        return u32::from_str_radix(stripped, 16).map(Some).map_err(|e| {
            anyhow!("E0019 sidecar `{path}` does not contain a hex literal (got {cleaned:?}): {e}")
        });
    }
    let Some((layer_name, sym)) = term.split_once('.') else {
        return Err(anyhow!(
            "E0006 patch term `{term}` is neither hex nor `<layer>.<symbol>`"
        ));
    };
    let load = plan
        .memory_loads
        .iter()
        .find(|m| m.layer == layer_name)
        .ok_or_else(|| {
            anyhow!(
                "E0006 patch term `{term}` references layer `{layer_name}` which has no memory load"
            )
        })?;
    if sym == "address" {
        return Ok(Some(load.address));
    }
    let _ = cfg;
    let entry = artifacts.by_layer.get(layer_name).ok_or_else(|| {
        anyhow!(
            "E0006 patch term `{term}` references layer `{layer_name}` whose artifact is not built"
        )
    })?;
    let offset = entry.listing.resolve(sym).ok_or_else(|| {
        anyhow!(
            "E0006 patch term `{term}` symbol `{sym}` not found in layer `{layer_name}`'s listing"
        )
    })?;
    Ok(Some(load.address.saturating_add(offset)))
}
