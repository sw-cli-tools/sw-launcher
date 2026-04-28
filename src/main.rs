//! Thin entry point for the `sw-launch` binary.
//!
//! Parses argv via clap, dispatches, and translates a returned
//! [`sw_launcher::Error`] into a user-visible message on stderr
//! plus the matching exit code.

use std::process::ExitCode;

use clap::Parser;
use sw_launcher::cli::{Cli, dispatch};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("sw-launch: {err}");
            let code = err.exit_code();
            // ExitCode::from takes a u8; clamp to that range.
            ExitCode::from(u8::try_from(code).unwrap_or(1))
        }
    }
}
