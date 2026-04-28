//! Error type for the sw-launch CLI.
//!
//! Each variant carries a stable error code from `docs/design.md`'s
//! "Validation rules" / "Error code stability" sections. New
//! variants are appended; existing codes never change meaning.
//!
//! Schema-validation codes E0001..E0027 are reserved for the
//! `validate` module (step 005); this enum currently models only
//! the runtime-shaped errors the scaffold needs.

use std::fmt;

/// Stable identifier for a user-visible diagnostic.
///
/// Format: `E0xxx` where `xxx` is a zero-padded decimal. The full
/// catalogue is in `docs/design.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCode(pub u16);

impl ErrorCode {
    /// `E0090`: action requested is implemented in a later step.
    pub const NOT_IMPLEMENTED: ErrorCode = ErrorCode(90);
    /// `E0091`: malformed CLI arguments not caught by clap itself.
    pub const CLI: ErrorCode = ErrorCode(91);
    /// `E0092`: I/O error reading or writing a file.
    pub const IO: ErrorCode = ErrorCode(92);
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{:04}", self.0)
    }
}

/// Top-level error type returned by `cli::dispatch`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A subcommand or option that the scaffold accepts but does not
    /// yet do anything useful with.
    #[error("{code} not yet implemented: {what}")]
    NotImplemented { code: ErrorCode, what: &'static str },

    /// Argument parsing or shape problems beyond what clap rejects.
    #[error("{code} {message}")]
    Cli { code: ErrorCode, message: String },

    /// Filesystem / process I/O wrapper.
    #[error("{} I/O: {source}", ErrorCode::IO)]
    Io {
        #[from]
        source: std::io::Error,
    },
}

impl Error {
    /// Construct a `NotImplemented` for a named subcommand.
    pub fn not_implemented(what: &'static str) -> Self {
        Error::NotImplemented {
            code: ErrorCode::NOT_IMPLEMENTED,
            what,
        }
    }

    /// Construct a `Cli` error from a string message.
    pub fn cli<S: Into<String>>(message: S) -> Self {
        Error::Cli {
            code: ErrorCode::CLI,
            message: message.into(),
        }
    }

    /// Numeric code suitable for an exit status.
    ///
    /// Always non-zero. Maps every variant to a small set so shell
    /// callers can distinguish argument errors from "not yet built"
    /// from I/O failures.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotImplemented { .. } => 2,
            Error::Cli { .. } => 64,
            Error::Io { .. } => 74,
        }
    }
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

// Tests moved to `tests/error_unit.rs` to keep module count under
// the sw-checklist crate-module budget.
