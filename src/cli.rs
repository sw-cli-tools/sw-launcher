//! Top-level CLI definitions and dispatch.
//!
//! Implements the surface from `docs/design.md` "CLI surface".
//! Every subcommand currently returns `Error::not_implemented`;
//! later steps replace each stub with the real action.

use clap::{Parser, Subcommand};

use crate::error::{Error, Result};

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
///
/// Currently every action returns `Error::not_implemented`. Later
/// steps replace each `not_implemented` call with the real handler.
pub fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Run { .. } => Err(Error::not_implemented("run")),
        Commands::Build { .. } => Err(Error::not_implemented("build")),
        Commands::Check { .. } => Err(Error::not_implemented("check")),
        Commands::Graph { .. } => Err(Error::not_implemented("graph")),
        Commands::Cache { action } => dispatch_cache(action),
        Commands::Vendor { action } => dispatch_vendor(action),
        Commands::Doctor => Err(Error::not_implemented("doctor")),
    }
}

fn dispatch_cache(action: CacheAction) -> Result<()> {
    match action {
        CacheAction::List => Err(Error::not_implemented("cache list")),
        CacheAction::Explain { .. } => Err(Error::not_implemented("cache explain")),
        CacheAction::Clean => Err(Error::not_implemented("cache clean")),
    }
}

fn dispatch_vendor(action: VendorAction) -> Result<()> {
    match action {
        VendorAction::Sync => Err(Error::not_implemented("vendor sync")),
        VendorAction::Status => Err(Error::not_implemented("vendor status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn assert_unimplemented(command: Commands, expected: &str) {
        let cli = Cli { command };
        let err = dispatch(cli).expect_err("should be unimplemented");
        match err {
            Error::NotImplemented { what, .. } => assert_eq!(what, expected),
            other => panic!("expected NotImplemented({expected}), got {other:?}"),
        }
    }

    fn s(name: &str) -> String {
        name.to_string()
    }

    #[test]
    fn clap_definition_validates() {
        Cli::command().debug_assert();
    }

    #[test]
    fn dispatch_returns_not_implemented_for_every_action() {
        let scen = || s("x");
        assert_unimplemented(Commands::Run { scenario: scen() }, "run");
        assert_unimplemented(Commands::Build { scenario: scen() }, "build");
        assert_unimplemented(Commands::Check { scenario: scen() }, "check");
        assert_unimplemented(Commands::Graph { scenario: scen() }, "graph");
        let list = Commands::Cache {
            action: CacheAction::List,
        };
        assert_unimplemented(list, "cache list");
        let explain = Commands::Cache {
            action: CacheAction::Explain { scenario: scen() },
        };
        assert_unimplemented(explain, "cache explain");
        let clean = Commands::Cache {
            action: CacheAction::Clean,
        };
        assert_unimplemented(clean, "cache clean");
        let sync = Commands::Vendor {
            action: VendorAction::Sync,
        };
        assert_unimplemented(sync, "vendor sync");
        let status = Commands::Vendor {
            action: VendorAction::Status,
        };
        assert_unimplemented(status, "vendor status");
        assert_unimplemented(Commands::Doctor, "doctor");
    }
}
