//! Unit tests for `sw_launcher::cli`. These were inline in the
//! source module until step 006 split them out to honor the
//! sw-checklist crate-module budget.

use clap::CommandFactory;
use sw_launcher::cli::{Cli, Commands, dispatch};
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
    // `Run`, `Build`, and `Check` are wired up as of step 006/007/009;
    // only the genuinely-stubbed actions remain in this assertion.
    let scen = || s("x");
    assert_unimplemented(Commands::Graph { scenario: scen() }, "graph");
    // Cache list/explain/clean are wired up as of Phase 4 step 2.
    // Vendor sync/status are wired up as of Phase 4 step 3.
    // Doctor is wired up as of Phase 4 step 4.
}
