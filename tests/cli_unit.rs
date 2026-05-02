//! Unit tests for `sw_launcher::cli`. These were inline in the
//! source module until step 006 split them out to honor the
//! sw-checklist crate-module budget.

use clap::CommandFactory;
use sw_launcher::cli::Cli;

#[test]
fn clap_definition_validates() {
    Cli::command().debug_assert();
}
