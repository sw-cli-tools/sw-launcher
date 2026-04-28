//! Top-level CLI definitions and dispatch.
//!
//! Implements the surface from `docs/design.md` "CLI surface".
//! Every subcommand currently returns `Error::not_implemented`;
//! later steps replace each stub with the real action.

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use crate::config::Config;
use crate::error::{Error, Result};
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
    },
    /// Build all layers of a scenario; do not execute.
    Build {
        /// Name of the scenario in `sw-launch.toml`.
        scenario: String,
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
        Commands::Run { .. } => Err(Error::not_implemented("run")),
        Commands::Build { .. } => Err(Error::not_implemented("build")),
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

/// Implementation of `sw-launch check <scenario>`. Loads the
/// config, runs `validate::validate`, prints every diagnostic to
/// stderr, returns Ok if the scenario validated cleanly.
fn check_scenario(config_path: &camino::Utf8Path, scenario: &str) -> Result<()> {
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
