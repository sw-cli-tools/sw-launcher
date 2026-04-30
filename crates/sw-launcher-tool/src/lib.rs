//! `sw-launcher-tool`: build-tool invocation, content-addressed
//! disk cache, and `cor24-run --assemble` listing parser.
//!
//! Extracted from the main `sw-launcher` crate in Phase 4 step 1 to
//! drop the binary crate's module count under sw-checklist's cap and
//! to give the disk cache its own home alongside the in-process
//! `Tool`.
//!
//! The public surface is the same as the prior `sw_launcher::tool`
//! module; the main crate re-exports it under that path so callers
//! see no change.

pub mod cache;
pub mod tool;
