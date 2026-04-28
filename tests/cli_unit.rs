//! Unit tests for `sw_launcher::cli`. These were inline in the
//! source module until step 006 split them out to honor the
//! sw-checklist crate-module budget.

use clap::CommandFactory;
use sw_launcher::cli::{CacheAction, Cli, Commands, VendorAction, dispatch};
use sw_launcher::error::Error;

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
fn dispatch_returns_not_implemented_for_unimplemented_actions() {
    // `Check` is wired up as of step 006; the rest still stub.
    let scen = || s("x");
    assert_unimplemented(Commands::Run { scenario: scen() }, "run");
    assert_unimplemented(Commands::Build { scenario: scen() }, "build");
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
