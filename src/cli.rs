//! Top-level CLI definitions and dispatch.
//!
//! Implements the surface from `docs/design.md` "CLI surface".
//! Every subcommand currently returns `Error::not_implemented`;
//! later steps replace each stub with the real action.

use std::collections::BTreeMap;
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};

use crate::config::{Config, Expect, Scenario};
use crate::error::{Error, Result};
use crate::manifest::{ArtifactEntry, Artifacts, LoadPlan};
use crate::tool::{Assembler, BuildJob, Listing, SourceSpec, Tool, ToolKind};
use crate::validate;

/// Long version string with copyright, license, repo, build host /
/// commit / time. Required by sw-checklist's "Version Field: ..."
/// rules. The build-info env vars are set by `build.rs`.
///
/// clap automatically prepends `<bin_name> ` to this string in
/// `--version` output, so we start with just the package version
/// rather than re-stating the binary name.
const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\n\nCopyright (c) 2026 Michael A. Wright",
    "\nLicense:    MIT (see LICENSE)",
    "\nRepository: ",
    env!("CARGO_PKG_REPOSITORY"),
    "\nBuild Host: ",
    env!("SW_LAUNCH_BUILD_HOST"),
    "\nBuild Commit: ",
    env!("SW_LAUNCH_BUILD_COMMIT"),
    "\nBuild Time: ",
    env!("SW_LAUNCH_BUILD_TIME"),
);

/// Long-form help shown by `--help`. Includes an AI CODING AGENT
/// INSTRUCTIONS section as required by sw-checklist.
const LONG_ABOUT: &str = "\
Host-side reproducible scenario launcher for COR24 emulator workflows.

sw-launch turns a layered emulator scenario, declared in
sw-launch.toml, into a single boring, reproducible command. It
resolves pinned dependency layers, builds artifacts (with caching),
assembles a deterministic memory load plan, invokes the target
emulator, and checks expected output.

The full schema, validation rules, and scenario shapes are in
docs/design.md. The 13-repo survey that grounds the schema is under
docs/survey/.

EXAMPLES
    sw-launch run my-scenario        Build, execute, check expectations.
    sw-launch check my-scenario      Validate config; do not run tools.
    sw-launch graph my-scenario      Print the layer DAG.
    sw-launch cache explain my-scen  Show cache hit/miss per layer.
    sw-launch vendor sync            Resolve and pin dependencies.
    sw-launch doctor                 Verify host tools are reachable.

AI CODING AGENT INSTRUCTIONS
    Agents working in repos that consume sw-launch should call
    `sw-launch run <scenario>` instead of hand-rolling cor24-run
    invocations with --load-binary, --patch, --entry, and
    --uart-input flags. The launcher exists specifically to remove
    that surface area: every layered emulator scenario is described
    once in sw-launch.toml and reused.

    Before doing anything else: run `sw-launch check <scenario>` to
    surface schema errors (E0001..E0027) without spawning the
    emulator. The diagnostic codes are stable; pattern-match on the
    code, not the prose.

    Cache discipline:
      - `--no-cache` forces rebuild of every layer
      - `--rebuild <layer>` rebuilds one layer, caches the rest
      - `cache explain <scenario>` shows per-layer hit/miss
      Never delete the cache to recover from a bug -- diagnose the
      stale-key root cause first.

    Lockfile discipline:
      - sw-launch.lock is committed to source. Treat it like
        Cargo.lock for binaries.
      - `vendor sync` updates it; `--update-lock` allows mutation
        on a single run.

    Reporting:
      - `--report-json <path>` emits a structured run report agents
        can parse without scraping prose.
      - `--explain` prints the resolved load plan; pair with
        `--dry-run` to inspect without invoking tools.

    Out of scope for sw-launch (do not paper over):
      - Filesystem stubs, GC scheduling, signal handling, multi-
        target sweeps in one config.

SEE ALSO
    docs/prd.md           Product requirements
    docs/architecture.md  Module layout and invariants
    docs/design.md        Schema and validation rules
    docs/plan.md          Phased rollout
    docs/status.md        Live status
    docs/survey/          Per-repo memory-layout surveys
    web/memory-layouts/   Browser visualization (./scripts/serve.sh)
";

/// `sw-launch` -- host-side reproducible scenario launcher.
#[derive(Debug, Parser)]
#[command(
    name = "sw-launch",
    // sw-checklist requires `-V` and `--version` to produce identical
    // output AND to include copyright / license / repo / build host /
    // commit / time fields. Use the long string for both.
    version = LONG_VERSION,
    about = "Host-side reproducible scenario launcher for COR24 emulator workflows",
    long_about = LONG_ABOUT,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Build (with cache) and execute a scenario; check expectations.
    Run {
        /// Name of the scenario in `sw-launch.toml`.
        scenario: String,
        /// Path to `sw-launch.toml` (default: ./sw-launch.toml).
        #[arg(short, long, default_value = "sw-launch.toml")]
        config: Utf8PathBuf,
    },
    /// Build all layers of a scenario; do not execute.
    Build {
        /// Name of the scenario in `sw-launch.toml`.
        scenario: String,
        /// Path to `sw-launch.toml` (default: ./sw-launch.toml).
        #[arg(short, long, default_value = "sw-launch.toml")]
        config: Utf8PathBuf,
    },
    /// Validate config + lockfile for a scenario; no tools spawned.
    Check {
        /// Name of the scenario in `sw-launch.toml`.
        scenario: String,
        /// Path to `sw-launch.toml` (default: ./sw-launch.toml).
        #[arg(short, long, default_value = "sw-launch.toml")]
        config: Utf8PathBuf,
    },
    /// Print the layer DAG for a scenario.
    Graph {
        /// Name of the scenario in `sw-launch.toml`.
        scenario: String,
    },
    /// Inspect or clean the build cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Resolve and inspect vendored dependencies.
    Vendor {
        #[command(subcommand)]
        action: VendorAction,
    },
    /// Verify that host tools (cor24-run, pa24r, ...) are reachable.
    Doctor,
}

/// `sw-launch cache <action>` subcommands.
#[derive(Debug, Subcommand)]
pub enum CacheAction {
    /// List cached artifacts.
    List,
    /// Show cache hit/miss for each layer of a scenario.
    Explain {
        /// Name of the scenario in `sw-launch.toml`.
        scenario: String,
    },
    /// Drop cache entries.
    Clean,
}

/// `sw-launch vendor <action>` subcommands.
#[derive(Debug, Subcommand)]
pub enum VendorAction {
    /// Resolve and pin all dependencies; write the lockfile.
    Sync,
    /// Compare TOML vs lockfile vs vendor store.
    Status,
}

/// Dispatch a parsed `Cli` to the appropriate handler.
pub fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Run { scenario, config } => run_scenario(&config, &scenario),
        Commands::Build { scenario, config } => build_scenario(&config, &scenario),
        Commands::Check { scenario, config } => check_scenario(&config, &scenario),
        Commands::Graph { .. } => Err(Error::not_implemented("graph")),
        Commands::Cache { action } => match action {
            CacheAction::List => Err(Error::not_implemented("cache list")),
            CacheAction::Explain { .. } => Err(Error::not_implemented("cache explain")),
            CacheAction::Clean => Err(Error::not_implemented("cache clean")),
        },
        Commands::Vendor { action } => match action {
            VendorAction::Sync => Err(Error::not_implemented("vendor sync")),
            VendorAction::Status => Err(Error::not_implemented("vendor status")),
        },
        Commands::Doctor => Err(Error::not_implemented("doctor")),
    }
}

/// `sw-launch run <scenario>` end to end:
/// validate -> assemble layers (with memoization) -> LoadPlan ->
/// spawn cor24-run -> capture UART -> check expectations.
fn run_scenario(config_path: &Utf8Path, scenario: &str) -> Result<()> {
    let cfg = Config::from_path(config_path)?;
    let scen = cfg
        .scenarios
        .get(scenario)
        .ok_or_else(|| Error::cli(format!("scenario `{scenario}` not declared")))?
        .clone();
    if let Err(diags) = validate::validate(&cfg, scenario) {
        for d in &diags {
            eprintln!("{d}");
        }
        return Err(Error::cli("validation failed; run aborted".to_string()));
    }
    let mut asm = Tool::from_source(
        &SourceSpec::FromPath {
            binary: "cor24-run".into(),
        },
        config_path.parent().unwrap_or_else(|| Utf8Path::new(".")),
        ToolKind::Assembler,
    )?;
    let artifacts = assemble_artifacts(&cfg, &scen, scenario, config_path, &mut asm)?;
    let cfg_dir = config_path.parent().unwrap_or_else(|| Utf8Path::new("."));
    let plan = LoadPlan::build(&cfg, scenario, &artifacts, cfg_dir)?;
    let (uart, exit_code) = run_emulator(&asm.tool_path, &plan, &scen)?;
    if let Some(expect) = &scen.expect {
        check_expectations(expect, &uart, exit_code)?;
    }
    println!("{uart}");
    Ok(())
}

/// Assemble (or pass through) every layer in `scenario`, returning
/// a map of name -> (artifact path, parsed listing). Dispatches on
/// `Layer.kind`:
///
/// - `"assembler"` -> Tool with ToolKind::Assembler (cor24-run).
/// - `"pcode"` -> Tool with ToolKind::Pcode (pa24r), lazy-
///   initialized on first encounter so scenarios that don't use
///   p-code never require pa24r on PATH.
/// - `"binary"` / `"pcode-image"` -> no build; the input file is
///   the artifact, with an empty listing.
///
/// Layer `input` paths are interpreted relative to the config
/// file's directory.
fn assemble_artifacts(
    cfg: &Config,
    scen: &Scenario,
    scenario_name: &str,
    config_path: &Utf8Path,
    asm: &mut Assembler,
) -> Result<Artifacts> {
    let cfg_dir: Utf8PathBuf = config_path
        .parent()
        .map(Utf8PathBuf::from)
        .unwrap_or_else(|| Utf8PathBuf::from("."));
    let out_root = cfg_dir.join(".sw-launch").join("build").join(scenario_name);
    let mut by_layer: BTreeMap<String, ArtifactEntry> = BTreeMap::new();
    let mut pcode: Option<Tool> = None;
    let mut pcode_linker: Option<Tool> = None;
    for layer_name in &scen.layers {
        let Some(layer) = cfg.layers.get(layer_name) else {
            continue;
        };
        let Some(input) = layer.input.as_deref() else {
            continue;
        };
        let resolved_input: Utf8PathBuf = if Utf8Path::new(input).is_absolute() {
            Utf8PathBuf::from(input)
        } else {
            cfg_dir.join(input)
        };
        match layer.kind.as_str() {
            "assembler" => {
                let layer_dir = out_root.join(layer_name);
                let job = BuildJob {
                    layer_name: layer_name.clone(),
                    input: resolved_input,
                    output_bin: layer_dir.join(format!("{layer_name}.bin")),
                    output_lst: layer_dir.join(format!("{layer_name}.lst")),
                    extra_args: Vec::new(),
                };
                let out = asm.build(&job)?;
                let lst_text =
                    std::fs::read_to_string(out.listing.as_std_path()).unwrap_or_default();
                by_layer.insert(
                    layer_name.clone(),
                    ArtifactEntry {
                        artifact: out.artifact,
                        listing: Listing::parse(&lst_text),
                    },
                );
            }
            "pcode" => {
                if pcode.is_none() {
                    pcode = Some(Tool::from_source(
                        &SourceSpec::FromPath {
                            binary: "pa24r".into(),
                        },
                        &cfg_dir,
                        ToolKind::Pcode,
                    )?);
                }
                let layer_dir = out_root.join(layer_name);
                // Phase 1: pa24r .spc -> .p24
                let p24 = layer_dir.join(format!("{layer_name}.p24"));
                let assemble_job = BuildJob {
                    layer_name: layer_name.clone(),
                    input: resolved_input,
                    output_bin: p24.clone(),
                    output_lst: Utf8PathBuf::new(),
                    extra_args: Vec::new(),
                };
                pcode.as_mut().unwrap().build(&assemble_job)?;
                // Phase 2: p24-load .p24 --load-addr <addr> -> .p24m
                let load_addr_hex = layer
                    .load
                    .as_ref()
                    .and_then(|l| l.address.as_ref())
                    .map(|h| h.0.clone())
                    .ok_or_else(|| {
                        Error::cli(format!(
                            "pcode layer `{layer_name}` needs load.address for p24-load"
                        ))
                    })?;
                if pcode_linker.is_none() {
                    pcode_linker = Some(Tool::from_source(
                        &SourceSpec::FromPath {
                            binary: "p24-load".into(),
                        },
                        &cfg_dir,
                        ToolKind::PcodeLinker,
                    )?);
                }
                let p24m = layer_dir.join(format!("{layer_name}.p24m"));
                let link_job = BuildJob {
                    layer_name: layer_name.clone(),
                    input: p24,
                    output_bin: p24m.clone(),
                    output_lst: Utf8PathBuf::new(),
                    extra_args: vec!["--load-addr".into(), load_addr_hex],
                };
                pcode_linker.as_mut().unwrap().build(&link_job)?;
                by_layer.insert(
                    layer_name.clone(),
                    ArtifactEntry {
                        artifact: p24m,
                        listing: Listing::default(),
                    },
                );
            }
            "binary" | "pcode-image" => {
                by_layer.insert(
                    layer_name.clone(),
                    ArtifactEntry {
                        artifact: resolved_input,
                        listing: Listing::default(),
                    },
                );
            }
            _ => continue,
        }
    }
    Ok(Artifacts { by_layer })
}

/// Spawn `cor24-run` with the resolved argv plus run-config
/// flags (`--time`, `-n`); capture stdout; extract UART and exit
/// code. Trusts cor24-run's `--time` for runtime budget; a
/// host-side wall timeout is step-010 work.
fn run_emulator(tool_path: &Utf8Path, plan: &LoadPlan, scen: &Scenario) -> Result<(String, i32)> {
    let mut cmd = Command::new(tool_path.as_std_path());
    cmd.args(plan.cor24_argv());
    cmd.args(["--speed", "0"]);
    if let Some(run) = &scen.run {
        if let Some(n) = run.max_cycles {
            cmd.args(["-n", &n.to_string()]);
        }
        if let Some(ms) = run.timeout_ms {
            let secs = (ms / 1000).max(1);
            cmd.args(["--time", &secs.to_string()]);
        }
    }
    let output = cmd.output().map_err(|source| Error::Io { source })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut uart = String::new();
    let mut in_block = false;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("UART output: ") {
            uart.push_str(rest);
            uart.push('\n');
            in_block = true;
        } else if line.starts_with("Executed ") {
            in_block = false;
        } else if in_block {
            uart.push_str(line);
            uart.push('\n');
        }
    }
    Ok((
        uart.trim_end().to_string(),
        output.status.code().unwrap_or(-1),
    ))
}

/// Run all expectation matchers; return Err(Cli) if any fail
/// with a human-readable mismatch report.
fn check_expectations(expect: &Expect, uart: &str, exit_code: i32) -> Result<()> {
    let mut mismatches: Vec<String> = Vec::new();
    for needle in &expect.uart_contains {
        if !uart.contains(needle) {
            mismatches.push(format!(
                "uart_contains: expected substring {needle:?} not found"
            ));
        }
    }
    for forbidden in &expect.uart_not_contains {
        if uart.contains(forbidden) {
            mismatches.push(format!(
                "uart_not_contains: forbidden substring {forbidden:?} appeared"
            ));
        }
    }
    for pat in &expect.uart_regex {
        match regex::Regex::new(pat) {
            Ok(re) => {
                if !re.is_match(uart) {
                    mismatches.push(format!("uart_regex: pattern {pat:?} did not match"));
                }
            }
            Err(e) => {
                mismatches.push(format!(
                    "uart_regex: pattern {pat:?} failed to compile: {e}"
                ));
            }
        }
    }
    if let Some(want) = expect.exit_code
        && want != exit_code
    {
        mismatches.push(format!("exit_code: expected {want}, got {exit_code}"));
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        for m in &mismatches {
            eprintln!("expectation mismatch: {m}");
        }
        eprintln!("--- captured UART ---");
        eprintln!("{uart}");
        eprintln!("--- end UART ---");
        Err(Error::cli(format!(
            "{} expectation(s) failed",
            mismatches.len()
        )))
    }
}

/// Implementation of `sw-launch build <scenario>`. Loads config,
/// reuses `assemble_artifacts` to dispatch on `Layer.kind`
/// (assembler / pcode / binary / pcode-image), prints each
/// produced artifact path. No emulator is spawned.
fn build_scenario(config_path: &Utf8Path, scenario: &str) -> Result<()> {
    let cfg = Config::from_path(config_path)?;
    let scen = cfg
        .scenarios
        .get(scenario)
        .ok_or_else(|| Error::cli(format!("scenario `{scenario}` not declared")))?
        .clone();
    let mut asm = Tool::from_source(
        &SourceSpec::FromPath {
            binary: "cor24-run".into(),
        },
        config_path.parent().unwrap_or_else(|| Utf8Path::new(".")),
        ToolKind::Assembler,
    )?;
    let artifacts = assemble_artifacts(&cfg, &scen, scenario, config_path, &mut asm)?;
    for (layer_name, entry) in &artifacts.by_layer {
        println!("built {layer_name} -> {}", entry.artifact);
    }
    Ok(())
}

/// Implementation of `sw-launch check <scenario>`. Loads the
/// config, runs `validate::validate`, prints every diagnostic to
/// stderr, returns Ok if the scenario validated cleanly.
fn check_scenario(config_path: &Utf8Path, scenario: &str) -> Result<()> {
    let cfg = Config::from_path(config_path)?;
    match validate::validate(&cfg, scenario) {
        Ok(warnings) => {
            for d in &warnings {
                eprintln!("{d}");
            }
            Ok(())
        }
        Err(diags) => {
            for d in &diags {
                eprintln!("{d}");
            }
            let count = diags
                .iter()
                .filter(|d| d.severity == validate::Severity::Error)
                .count();
            Err(Error::cli(format!("validation failed: {count} error(s)")))
        }
    }
}

// Tests moved to `tests/cli_unit.rs` to keep module count under
// the sw-checklist crate-module budget. CLI integration tests
// remain in `tests/cli.rs`.
