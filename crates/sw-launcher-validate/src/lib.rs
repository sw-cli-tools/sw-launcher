//! Cross-field validation for `sw-launch.toml`, schema v1.2.
//!
//! Each rule has a stable error code (E0001..E0034). The full
//! catalogue is in `docs/design.md` "Validation rules". The codes
//! never change meaning; rules added later get new codes.
//!
//! `validate(&Config, scenario_name)` collects every diagnostic
//! before returning. Callers (e.g. `cli::check`) print everything
//! and exit non-zero if any error-severity diagnostic was emitted.
//!
//! Rules that require artifact sizes (overlap of byte ranges
//! resolved through listings) or vendor lockfile state (E0019
//! sidecar staleness) are stubbed at this step and re-enabled by
//! later steps as their infrastructure lands.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sw_launcher_config::{
    Config, ErrorCode, HeapCategory, Layer, LoadMethod, MemRange, MemoryProfile, Patch, Scenario,
    Segment, SegmentKind, SizeOrAuto, Target,
};

/// Severity for a diagnostic. Errors fail the run; warnings are
/// reported but do not change exit status unless `--strict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A single validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: ErrorCode,
    pub severity: Severity,
    pub layer: Option<String>,
    pub segment: Option<String>,
    pub message: String,
    pub hint: Option<String>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{sev}[{}]: {}", self.code, self.message)?;
        if let Some(layer) = &self.layer {
            write!(f, "\n  layer: {layer}")?;
        }
        if let Some(segment) = &self.segment {
            write!(f, "\n  segment: {segment}")?;
        }
        if let Some(hint) = &self.hint {
            write!(f, "\n  hint: {hint}")?;
        }
        Ok(())
    }
}

impl Diagnostic {
    fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            layer: None,
            segment: None,
            message: message.into(),
            hint: None,
        }
    }
    fn warning(code: ErrorCode, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Warning,
            layer: None,
            segment: None,
            message: message.into(),
            hint: None,
        }
    }
    fn with_layer(mut self, layer: impl Into<String>) -> Self {
        self.layer = Some(layer.into());
        self
    }
    fn with_segment(mut self, segment: Option<&str>) -> Self {
        self.segment = segment.map(String::from);
        self
    }
    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Validate a single scenario. Returns `Ok(warnings)` when there
/// are no errors; otherwise `Err(all_diagnostics)` so callers can
/// print the full list and exit non-zero on errors only.
pub fn validate(cfg: &Config, scenario_name: &str) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    let mut out = Vec::new();
    let scen = match cfg.scenarios.get(scenario_name) {
        Some(s) => s,
        None => {
            out.push(Diagnostic::error(
                ErrorCode(2),
                format!("scenario `{scenario_name}` not declared"),
            ));
            return Err(out);
        }
    };
    rules::check_scenario(cfg, scenario_name, scen, &mut out);
    rules::check_layers(cfg, scen, &mut out);
    rules::check_profile(cfg, scenario_name, scen, &mut out);
    rules::check_segments(cfg, scen, &mut out);
    rules::check_overlap(cfg, scen, &mut out);
    rules::check_heap_rules(cfg, scen, &mut out);
    rules::check_total_budget(cfg, scen, &mut out);

    if out.iter().any(|d| d.severity == Severity::Error) {
        Err(out)
    } else {
        Ok(out)
    }
}

/// Inline submodule scopes the rule functions so the function
/// count is split between this file's "module" and the inner
/// `rules` module per sw-checklist's accounting.
mod rules {
    use super::*;

    /// E0001 (no layers), E0002 (target undeclared), E0008 (no
    /// halt_on/max_cycles).
    pub(super) fn check_scenario(
        cfg: &Config,
        scenario_name: &str,
        scen: &Scenario,
        out: &mut Vec<Diagnostic>,
    ) {
        if scen.layers.is_empty() {
            out.push(Diagnostic::error(
                ErrorCode(1),
                format!("scenario `{scenario_name}` declares no layers"),
            ));
        }
        let run = scen.run.as_ref();
        if run.and_then(|r| r.halt_on.as_ref()).is_none()
            && run.and_then(|r| r.max_cycles).is_none()
        {
            out.push(
                Diagnostic::error(
                    ErrorCode(8),
                    format!("scenario `{scenario_name}` has no halt_on or max_cycles"),
                )
                .with_hint("set [scenarios.<name>.run].halt_on or .max_cycles"),
            );
        }
        if !cfg.targets.contains_key(&scen.target) {
            out.push(Diagnostic::error(
                ErrorCode(2),
                format!(
                    "scenario `{scenario_name}` references undeclared target `{}`",
                    scen.target
                ),
            ));
        }
    }

    /// Per-layer rules: E0002 ref, E0004 uart, E0005 kind/load,
    /// E0006 patch shape (literal hex AND cross-layer
    /// `<layer>.<symbol>`), E0015 partial, E0016 self.* outside
    /// segment.
    pub(super) fn check_layers(cfg: &Config, scen: &Scenario, out: &mut Vec<Diagnostic>) {
        for name in &scen.layers {
            let Some(layer) = cfg.layers.get(name) else {
                out.push(Diagnostic::error(
                    ErrorCode(2),
                    format!("scenario references undeclared layer `{name}`"),
                ));
                continue;
            };
            check_kind_load(name, layer, out);
            check_uart_size(name, layer, out);
            for p in &layer.patches {
                check_patch(name, p, cfg, out);
            }
        }
    }

    fn check_kind_load(name: &str, layer: &Layer, out: &mut Vec<Diagnostic>) {
        let Some(load) = &layer.load else { return };
        let kind = layer.kind.as_str();
        let bad_uart = matches!(
            kind,
            "assembler" | "binary" | "pcode" | "pcode-image" | "composite"
        ) && load.method == LoadMethod::Uart;
        if bad_uart {
            out.push(
                Diagnostic::error(
                    ErrorCode(5),
                    format!("kind `{kind}` cannot use load.method = \"uart\""),
                )
                .with_layer(name),
            );
        }
        if load.method == LoadMethod::Uart && load.max_bytes.is_none() {
            out.push(
                Diagnostic::error(ErrorCode(4), "uart layer is missing load.max_bytes")
                    .with_layer(name)
                    .with_hint("set load.max_bytes = <hard cap>"),
            );
        }
        if load.method == LoadMethod::Memory
            && load.address.is_none()
            && !layer.absolute_addresses
            && layer.segments.is_empty()
        {
            out.push(
                Diagnostic::error(ErrorCode(15), "memory load with no address and no segments")
                    .with_layer(name)
                    .with_hint("set load.address or declare segments with claims"),
            );
        }
    }

    fn check_uart_size(name: &str, layer: &Layer, out: &mut Vec<Diagnostic>) {
        let Some(load) = &layer.load else { return };
        if load.method != LoadMethod::Uart {
            return;
        }
        let Some(max_bytes) = load.max_bytes else {
            return;
        };
        if let Some(input) = &layer.input
            && let Ok(meta) = std::fs::metadata(input)
            && meta.len() > max_bytes
        {
            out.push(
                Diagnostic::error(
                    ErrorCode(4),
                    format!(
                        "uart input `{input}` ({} bytes) exceeds max_bytes ({max_bytes})",
                        meta.len()
                    ),
                )
                .with_layer(name),
            );
        }
    }

    fn check_patch(name: &str, p: &Patch, cfg: &Config, out: &mut Vec<Diagnostic>) {
        if p.target.starts_with("0x") && sw_launcher_config::parse_hex_u32(&p.target).is_err() {
            out.push(
                Diagnostic::error(
                    ErrorCode(6),
                    format!("patch target `{}` is not a valid hex address", p.target),
                )
                .with_layer(name),
            );
        }
        let v = p.value.as_str();
        if v == "self.address" || v == "self.end" || v == "self.size" {
            out.push(
                Diagnostic::error(
                    ErrorCode(16),
                    format!("patch value `{v}` only valid inside a segment block"),
                )
                .with_layer(name)
                .with_hint("move this patch into a [[layers.<n>.segments]] entry"),
            );
        } else if v.starts_with("0x") && sw_launcher_config::parse_hex_u32(v).is_err() {
            out.push(
                Diagnostic::error(
                    ErrorCode(6),
                    format!("patch value `{v}` is not a valid hex literal"),
                )
                .with_layer(name),
            );
        }
        check_patch_term(&p.target, name, cfg, out);
        check_patch_term(&p.value, name, cfg, out);
    }

    /// E0006 cross-layer shape check for a patch term (target or
    /// value). Verifies that `<other>.<symbol>` references a
    /// declared layer with an exports block listing the symbol;
    /// surfaces a Levenshtein "did you mean ...?" hint when not.
    /// `<other>.address` is special-cased and only requires the
    /// layer to exist.
    fn check_patch_term(term: &str, layer_name: &str, cfg: &Config, out: &mut Vec<Diagnostic>) {
        if term.starts_with("0x")
            || term.starts_with("sidecar:")
            || term == "self.address"
            || term == "self.end"
            || term == "self.size"
        {
            return;
        }
        let Some((other, sym)) = term.split_once('.') else {
            return;
        };
        let Some(other_layer) = cfg.layers.get(other) else {
            let names: Vec<String> = cfg.layers.keys().cloned().collect();
            let hint = closest_match(other, &names)
                .map(|s| format!("did you mean `{s}.{sym}`?"))
                .unwrap_or_else(|| format!("declared layers: {names:?}"));
            out.push(
                Diagnostic::error(
                    ErrorCode(6),
                    format!("patch term `{term}` references undeclared layer `{other}`"),
                )
                .with_layer(layer_name)
                .with_hint(hint),
            );
            return;
        };
        if sym == "address" {
            return;
        }
        let Some(exports) = &other_layer.exports else {
            out.push(
                Diagnostic::error(
                    ErrorCode(6),
                    format!(
                        "patch term `{term}` requires layer `{other}` to declare \
                         `exports.symbols = [\"{sym}\", ...]`"
                    ),
                )
                .with_layer(layer_name)
                .with_hint(format!(
                    "add [layers.{other}.exports] symbols = [\"{sym}\"]"
                )),
            );
            return;
        };
        if !exports.symbols.iter().any(|s| s == sym) {
            let hint = closest_match(sym, &exports.symbols)
                .map(|s| format!("did you mean `{other}.{s}`?"))
                .unwrap_or_else(|| format!("declared symbols: {:?}", exports.symbols));
            out.push(
                Diagnostic::error(
                    ErrorCode(6),
                    format!(
                        "patch term `{term}` symbol `{sym}` not in `{other}`'s exports.symbols"
                    ),
                )
                .with_layer(layer_name)
                .with_hint(hint),
            );
        }
    }

    /// Wagner-Fischer Levenshtein distance, returning the closest
    /// candidate within edit distance 3. Returns `None` if no
    /// candidate is close enough -- avoids "did you mean ANY"
    /// suggestions over wildly different strings.
    fn closest_match<'a>(needle: &str, haystack: &'a [String]) -> Option<&'a str> {
        let best = haystack
            .iter()
            .map(|h| (h.as_str(), edit_distance(needle, h)))
            .min_by_key(|(_, d)| *d);
        match best {
            Some((s, d)) if d <= 3 => Some(s),
            _ => None,
        }
    }

    fn edit_distance(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut dp: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.iter().enumerate() {
            let mut prev = dp[0];
            dp[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let cur = dp[j + 1];
                let sub = if ca == cb { prev } else { prev + 1 };
                dp[j + 1] = sub.min(dp[j + 1] + 1).min(dp[j] + 1);
                prev = cur;
            }
        }
        dp[b.len()]
    }

    /// E0011 (segment region check), E0013 (size>0), E0014
    /// (embedded->symbol), E0015 (reserved->size+addr/claims).
    pub(super) fn check_segments(cfg: &Config, scen: &Scenario, out: &mut Vec<Diagnostic>) {
        let target = cfg.targets.get(&scen.target);
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            for seg in &layer.segments {
                check_seg_size(layer_name, seg, out);
                check_seg_embedded(layer_name, seg, out);
                check_seg_in_target(layer_name, seg, target, out);
            }
        }
    }

    fn check_seg_size(layer_name: &str, seg: &Segment, out: &mut Vec<Diagnostic>) {
        if !matches!(
            seg.kind,
            SegmentKind::Stack | SegmentKind::Heap | SegmentKind::Bss
        ) {
            return;
        }
        let nonzero = match &seg.size {
            Some(SizeOrAuto::Hex(h)) if h.0 == "auto" => true,
            Some(SizeOrAuto::Hex(h)) => h.as_u32().map(|v| v > 0).unwrap_or(false),
            None => false,
        };
        if !nonzero {
            out.push(
                Diagnostic::error(
                    ErrorCode(13),
                    "stack/heap/bss segment must have nonzero size",
                )
                .with_layer(layer_name)
                .with_segment(seg.name.as_deref()),
            );
        }
    }

    fn check_seg_embedded(layer_name: &str, seg: &Segment, out: &mut Vec<Diagnostic>) {
        match seg.embedded {
            Some(true) => {
                if seg.symbol.is_none() {
                    out.push(
                        Diagnostic::error(
                            ErrorCode(14),
                            "embedded = true segment must declare `symbol`",
                        )
                        .with_layer(layer_name)
                        .with_segment(seg.name.as_deref())
                        .with_hint("set symbol = \"<label-in-source>\""),
                    );
                }
            }
            Some(false) => {
                let has_addr = seg.load.as_ref().and_then(|l| l.address.as_ref()).is_some();
                let has_claims = !seg.claims.is_empty();
                let has_size = seg.size.is_some();
                if !has_size {
                    out.push(
                        Diagnostic::error(
                            ErrorCode(15),
                            "embedded = false segment must declare `size`",
                        )
                        .with_layer(layer_name)
                        .with_segment(seg.name.as_deref()),
                    );
                }
                if !has_addr && !has_claims {
                    out.push(
                        Diagnostic::error(
                            ErrorCode(15),
                            "embedded = false segment needs load.address or claims = [...]",
                        )
                        .with_layer(layer_name)
                        .with_segment(seg.name.as_deref()),
                    );
                }
            }
            None => {}
        }
    }

    fn check_seg_in_target(
        layer_name: &str,
        seg: &Segment,
        target: Option<&Target>,
        out: &mut Vec<Diagnostic>,
    ) {
        let Some(target) = target else { return };
        let Some(load) = &seg.load else { return };
        let Some(addr_hex) = &load.address else {
            return;
        };
        let Ok(addr) = addr_hex.as_u32() else { return };
        let Some(end) = seg
            .size
            .as_ref()
            .and_then(SizeOrAuto::as_hex)
            .and_then(|h| h.as_u32().ok())
            .map(|sz| addr + sz)
        else {
            return;
        };
        let r = &target.regions;
        if !inside(addr, end, &r.sram)
            || overlaps(addr, end, &r.ebr_stack)
            || overlaps(addr, end, &r.mmio)
        {
            out.push(
                Diagnostic::error(
                    ErrorCode(11),
                    format!(
                        "segment range [0x{addr:06X}..0x{end:06X}) escapes SRAM or touches reserved regions"
                    ),
                )
                .with_layer(layer_name)
                .with_segment(seg.name.as_deref()),
            );
        }
    }

    fn inside(addr: u32, end: u32, range: &MemRange) -> bool {
        let lo = range.start.as_u32().unwrap_or(0);
        let hi = range.end.as_u32().unwrap_or(0);
        addr >= lo && end <= hi.saturating_add(1)
    }

    fn overlaps(addr: u32, end: u32, range: &MemRange) -> bool {
        let lo = range.start.as_u32().unwrap_or(0);
        let hi = range.end.as_u32().unwrap_or(0);
        !(end <= lo || addr > hi)
    }

    /// E0003 cross-layer overlap. Collects ranges that have
    /// declarable sizes (binary-kind layers with explicit
    /// `size`, plus any non-embedded segment with
    /// `load.address` + `size`) and reports any pair whose
    /// ranges intersect. Embedded segments and assembler/
    /// pcode kinds without explicit `size` are skipped --
    /// their byte ranges depend on artifact size which the
    /// validator doesn't read.
    pub(super) fn check_overlap(cfg: &Config, scen: &Scenario, out: &mut Vec<Diagnostic>) {
        let mut ranges: Vec<(String, String, u32, u32)> = Vec::new();
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            // Layer-level binary range (when layer.size declared).
            if let (Some(load), Some(SizeOrAuto::Hex(sz))) = (&layer.load, &layer.size)
                && load.method == LoadMethod::Memory
                && let Some(addr_hex) = &load.address
                && let (Ok(addr), Ok(size)) = (addr_hex.as_u32(), sz.as_u32())
                && size > 0
            {
                ranges.push((
                    layer_name.clone(),
                    "<image>".into(),
                    addr,
                    addr.saturating_add(size),
                ));
            }
            // Non-embedded segment ranges.
            for seg in &layer.segments {
                if seg.embedded == Some(true) {
                    continue;
                }
                let Some(load) = &seg.load else { continue };
                let Some(addr_hex) = &load.address else {
                    continue;
                };
                let Some(SizeOrAuto::Hex(size_hex)) = &seg.size else {
                    continue;
                };
                let (Ok(addr), Ok(size)) = (addr_hex.as_u32(), size_hex.as_u32()) else {
                    continue;
                };
                if size == 0 {
                    continue;
                }
                ranges.push((
                    layer_name.clone(),
                    seg.name.clone().unwrap_or_else(|| "<segment>".into()),
                    addr,
                    addr.saturating_add(size),
                ));
            }
        }
        for i in 0..ranges.len() {
            for j in (i + 1)..ranges.len() {
                let (la, na, sa, ea) = (&ranges[i].0, &ranges[i].1, ranges[i].2, ranges[i].3);
                let (lb, nb, sb, eb) = (&ranges[j].0, &ranges[j].1, ranges[j].2, ranges[j].3);
                if !(eb <= sa || ea <= sb) {
                    out.push(Diagnostic::error(
                        ErrorCode(3),
                        format!(
                            "layer `{la}` ({na}, [0x{sa:06X}..0x{ea:06X})) overlaps \
                             layer `{lb}` ({nb}, [0x{sb:06X}..0x{eb:06X}))"
                        ),
                    ));
                }
            }
        }
    }

    /// E0031 (undeclared profile), E0032 (region-not-in-profile),
    /// E0033 (profile self-overlap).
    pub(super) fn check_profile(
        cfg: &Config,
        scenario_name: &str,
        scen: &Scenario,
        out: &mut Vec<Diagnostic>,
    ) {
        let Some(prof_name) = scen.memory_profile.as_ref() else {
            return;
        };
        let Some(profile) = cfg.memory_profiles.get(prof_name) else {
            out.push(
                Diagnostic::error(
                    ErrorCode(31),
                    format!(
                        "scenario `{scenario_name}` references undeclared memory_profile `{prof_name}`"
                    ),
                )
                .with_hint(format!(
                    "declare [memory_profiles.{prof_name}] or pick another profile"
                )),
            );
            return;
        };
        check_profile_self_overlap(prof_name, profile, out);
        check_layer_claims(cfg, scen, profile, out);
    }

    fn check_profile_self_overlap(
        prof_name: &str,
        profile: &MemoryProfile,
        out: &mut Vec<Diagnostic>,
    ) {
        let mut ranges: Vec<(String, u32, u32)> = Vec::new();
        for p in &profile.partitions {
            let (Ok(base), Ok(size)) = (p.base.as_u32(), p.size.as_u32()) else {
                continue;
            };
            ranges.push((p.name.clone(), base, base.saturating_add(size)));
        }
        ranges.sort_by_key(|r| r.1);
        for w in ranges.windows(2) {
            if w[1].1 < w[0].2 {
                out.push(Diagnostic::error(
                    ErrorCode(33),
                    format!(
                        "memory_profile `{prof_name}` partitions `{}` and `{}` overlap",
                        w[0].0, w[1].0
                    ),
                ));
            }
        }
    }

    fn check_layer_claims(
        cfg: &Config,
        scen: &Scenario,
        profile: &MemoryProfile,
        out: &mut Vec<Diagnostic>,
    ) {
        let mut available: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for p in &profile.partitions {
            let regs: BTreeSet<&str> = p.regions.iter().map(|r| r.name.as_str()).collect();
            available.insert(p.name.as_str(), regs);
        }
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            if layer.absolute_addresses {
                continue;
            }
            for seg in &layer.segments {
                for claim in &seg.claims {
                    let (Some(part), Some(region)) = (&claim.partition, &claim.region) else {
                        continue;
                    };
                    let ok = available
                        .get(part.as_str())
                        .map(|rs| rs.contains(region.as_str()))
                        .unwrap_or(false);
                    if !ok {
                        out.push(
                            Diagnostic::error(
                                ErrorCode(32),
                                format!(
                                    "claim ({part}, {region}) not in profile partitions/regions"
                                ),
                            )
                            .with_layer(layer_name)
                            .with_segment(seg.name.as_deref())
                            .with_hint(format!("available: {available:?}")),
                        );
                    }
                }
            }
        }
    }

    /// E0028 / E0029 budget overshoot + 80% warn; E0030 missing
    /// justification or dead-leak/algorithmic-bloat warning.
    pub(super) fn check_heap_rules(cfg: &Config, scen: &Scenario, out: &mut Vec<Diagnostic>) {
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            check_layer_heap(layer_name, layer, out);
        }
        budget_overshoot(cfg, scen, out);
    }

    fn check_layer_heap(layer_name: &str, layer: &Layer, out: &mut Vec<Diagnostic>) {
        let layer_heap = layer_heap_size(layer);
        if layer_heap > 32 * 1024 && layer.heap_justification.is_none() {
            out.push(
                Diagnostic::error(
                    ErrorCode(30),
                    format!(
                        "layer claims heap > 32 KiB ({layer_heap} bytes) without heap_justification"
                    ),
                )
                .with_layer(layer_name)
                .with_hint("add [layers.<n>.heap_justification] with category + note"),
            );
        }
        if let Some(j) = &layer.heap_justification
            && matches!(
                j.category,
                HeapCategory::DeadLeak | HeapCategory::AlgorithmicBloat
            )
        {
            out.push(
                Diagnostic::warning(
                    ErrorCode(30),
                    format!(
                        "heap_justification.category = {:?}; --strict will reject",
                        j.category
                    ),
                )
                .with_layer(layer_name)
                .with_hint("address the underlying issue (GC, refactor) instead"),
            );
        }
    }

    fn budget_overshoot(cfg: &Config, scen: &Scenario, out: &mut Vec<Diagnostic>) {
        let Some(prof_name) = scen.memory_profile.as_ref() else {
            return;
        };
        let Some(profile) = cfg.memory_profiles.get(prof_name) else {
            return;
        };
        let Some(budget) = profile.budget.as_ref() else {
            return;
        };
        let Ok(budget_max) = budget.heap_max.as_u32() else {
            return;
        };
        let bm = u64::from(budget_max);
        let mut total: u64 = 0;
        for layer_name in &scen.layers {
            if let Some(layer) = cfg.layers.get(layer_name) {
                total += u64::from(layer_heap_size(layer));
            }
        }
        if total > bm {
            out.push(Diagnostic::error(
                ErrorCode(28),
                format!("scenario heap claims ({total} bytes) exceed budget.heap_max ({bm} bytes)"),
            ));
        } else if total * 5 > bm * 4 {
            out.push(
                Diagnostic::warning(
                    ErrorCode(29),
                    format!(
                        "scenario heap claims ({total} bytes) above 80% of budget ({bm} bytes)"
                    ),
                )
                .with_hint("verify the layer is not silently bloating"),
            );
        }
    }

    fn layer_heap_size(layer: &Layer) -> u32 {
        let mut total: u32 = 0;
        for seg in &layer.segments {
            if !matches!(seg.kind, SegmentKind::Heap) {
                continue;
            }
            if let Some(h) = seg.size.as_ref().and_then(SizeOrAuto::as_hex) {
                total = total.saturating_add(h.as_u32().unwrap_or(0));
            }
        }
        total
    }

    /// E0034 1 MiB rule of thumb.
    pub(super) fn check_total_budget(cfg: &Config, scen: &Scenario, out: &mut Vec<Diagnostic>) {
        let one_mib: u64 = 0x100000;
        let mut total: u64 = 0;
        for layer_name in &scen.layers {
            let Some(layer) = cfg.layers.get(layer_name) else {
                continue;
            };
            if layer.acknowledge_oversized {
                continue;
            }
            for seg in &layer.segments {
                if let Some(h) = seg.size.as_ref().and_then(SizeOrAuto::as_hex) {
                    total = total.saturating_add(u64::from(h.as_u32().unwrap_or(0)));
                }
            }
        }
        if total > one_mib {
            out.push(
                Diagnostic::warning(
                    ErrorCode(34),
                    format!("scenario claims {total} bytes total; rule of thumb is <= 1 MiB"),
                )
                .with_hint("set acknowledge_oversized = true on the offending layer to silence"),
            );
        }
    }
}
