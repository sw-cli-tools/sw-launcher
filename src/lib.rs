//! Library crate for sw-launcher.
//!
//! The binary at `src/main.rs` is a thin wrapper around
//! [`cli::dispatch`]. Tests live alongside the modules they cover.
//!
//! Public surface today is intentionally small: `cli` and `error`.
//! Later steps add `config`, `validate`, `manifest`, `target`,
//! `tool`, `run`, and `expect`.

pub mod cli;
pub mod error;

pub use error::{Error, ErrorCode, Result};
