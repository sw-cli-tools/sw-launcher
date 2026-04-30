//! Library crate for sw-launcher.
//!
//! The binary at `src/main.rs` is a thin wrapper around
//! [`cli::dispatch`]. Tests live alongside the modules they cover.
//!
//! Phase 4 step 1 extracted `tool` into the `sw-launcher-tool`
//! sub-crate (alongside the new disk `cache`). The re-exports
//! below preserve the prior `sw_launcher::tool::*` surface so
//! callers and tests don't need to update their import paths.

pub mod cli;
pub mod config;
pub mod error;
pub mod manifest;
pub mod validate;

pub use sw_launcher_tool::cache;
pub use sw_launcher_tool::tool;

/// Re-export of the listing parser at the crate root for symmetry
/// with `tool::Assembler`.
pub mod listing {
    pub use crate::tool::Listing;
}

pub use error::{Error, ErrorCode, Result};
