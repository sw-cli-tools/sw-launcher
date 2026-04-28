//! Library crate for sw-launcher.
//!
//! The binary at `src/main.rs` is a thin wrapper around
//! [`cli::dispatch`]. Tests live alongside the modules they cover.
//!
//! Public surface today is intentionally small: `cli` and `error`.
//! Later steps add `config`, `validate`, `manifest`, `target`,
//! `tool`, `run`, and `expect`.

pub mod cli;
pub mod config;
pub mod error;
pub mod manifest;
pub mod tool;
pub mod validate;

/// Re-export of the listing parser at the crate root for symmetry
/// with `tool::Assembler`. Both live in `tool` (combined to honor
/// sw-checklist's crate-module budget) but readers may expect
/// `sw_launcher::listing` to exist by name.
pub mod listing {
    pub use crate::tool::Listing;
}

pub use error::{Error, ErrorCode, Result};
