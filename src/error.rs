//! Error type for the sw-launch CLI.
//!
//! Each variant carries a stable error code from `docs/design.md`'s
//! "Validation rules" / "Error code stability" sections. New
//! variants are appended; existing codes never change meaning.
//!
//! Schema-validation codes E0001..E0027 are reserved for the
//! `validate` module (step 005); this enum currently models only
//! the runtime-shaped errors the scaffold needs.

// Phase 5 step 1: ErrorCode moved into sw-launcher-config so it
// can be shared between the validate sub-crate and the main
// crate's error type without a circular dependency.
pub use sw_launcher_config::ErrorCode;

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

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Cli {
            code: ErrorCode::CLI,
            message: format!("{e:#}"),
        }
    }
}

// Tests moved to `tests/error_unit.rs` to keep module count under
// the sw-checklist crate-module budget.
