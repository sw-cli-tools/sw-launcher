//! Build-tool invocation, per-process memoization, and listing
//! parsing.
//!
//! Step 007 ships exactly one tool, the COR24 assembler (which is
//! the same `cor24-run` binary invoked with `--assemble`). Later
//! steps add p-code assembler / linker / composite linker; the
//! shape is meant to extend.
//!
//! The listing parser also lives here: every assembler run emits
//! a `.lst` whose `<symbol>:` lines carry the absolute address
//! later steps need for `code_ptr`-style patches. Keeping
//! parser + assembler colocated keeps the crate at <= 7 modules
//! per sw-checklist's cap.
//!
//! The cache is in-memory and per-process: a single `sw-launch`
//! invocation that needs the same artifact twice spawns the tool
//! exactly once. Disk-persistent caching is Phase 4 work
//! (step 010+).

use std::collections::{BTreeMap, HashMap};
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// One build request: input source, where to write outputs,
/// extra command-line arguments to forward to the tool.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildJob {
    pub layer_name: String,
    pub input: Utf8PathBuf,
    pub output_bin: Utf8PathBuf,
    pub output_lst: Utf8PathBuf,
    pub extra_args: Vec<String>,
}

/// Result of one successful build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutput {
    pub artifact: Utf8PathBuf,
    pub listing: Utf8PathBuf,
    pub stdout: String,
    pub stderr: String,
}

/// Wraps `cor24-run --assemble <input> <out.bin> <out.lst>`.
pub struct Assembler {
    /// The host binary (`cor24-run`). Public so callers can reuse
    /// the same path for the emulator-spawn side of step 009 without
    /// re-resolving.
    pub tool_path: Utf8PathBuf,
    cache: HashMap<CacheKey, BuildOutput>,
    /// Number of times the tool has actually been spawned by this
    /// `Assembler` instance. Tests use it to verify memoization.
    pub spawn_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    tool_path: String,
    input_sha: String,
    args: Vec<String>,
}

impl Assembler {
    /// Create an assembler that invokes the binary at `tool_path`.
    pub fn new(tool_path: impl Into<Utf8PathBuf>) -> Self {
        Assembler {
            tool_path: tool_path.into(),
            cache: HashMap::new(),
            spawn_count: 0,
        }
    }

    /// Find `cor24-run` on `$PATH`. Convenience for callers that
    /// haven't read a `[tools.assembler.source]` block from TOML.
    pub fn from_path() -> Result<Self> {
        let paths = std::env::var_os("PATH").ok_or_else(|| Error::cli("PATH unset"))?;
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("cor24-run");
            if candidate.is_file() {
                let utf8 = Utf8PathBuf::from_path_buf(candidate)
                    .map_err(|p| Error::cli(format!("non-UTF8 cor24-run path: {p:?}")))?;
                return Ok(Self::new(utf8));
            }
        }
        Err(Error::cli("cor24-run not found on PATH"))
    }

    /// Build the requested artifact. Hits the in-process cache if
    /// the same inputs+args have been built already; otherwise
    /// spawns `cor24-run --assemble` and records the result.
    pub fn build(&mut self, job: &BuildJob) -> Result<BuildOutput> {
        let key = CacheKey {
            tool_path: self
                .tool_path
                .canonicalize_utf8()
                .unwrap_or_else(|_| self.tool_path.clone())
                .to_string(),
            input_sha: hash_file_sha256(&job.input)?,
            args: job.extra_args.clone(),
        };
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        for parent in [job.output_bin.parent(), job.output_lst.parent()]
            .into_iter()
            .flatten()
        {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io { source })?;
        }
        let mut cmd = Command::new(self.tool_path.as_std_path());
        cmd.arg("--assemble")
            .arg(job.input.as_std_path())
            .arg(job.output_bin.as_std_path())
            .arg(job.output_lst.as_std_path());
        for a in &job.extra_args {
            cmd.arg(a);
        }
        let output = cmd.output().map_err(|source| Error::Io { source })?;
        self.spawn_count += 1;
        if !output.status.success() {
            return Err(Error::cli(format!(
                "{} --assemble exited with {}: {}",
                self.tool_path,
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let out = BuildOutput {
            artifact: job.output_bin.clone(),
            listing: job.output_lst.clone(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        self.cache.insert(key, out.clone());
        Ok(out)
    }
}

/// Compute the SHA-256 of a file's contents, hex-encoded.
pub fn hash_file_sha256(path: &Utf8Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io { source })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

// ---------- Listing parser -----------------------------------------

/// Parsed view of a `cor24-run --assemble` `.lst` file: maps each
/// emitted symbol to its absolute address.
///
/// Lines come in two shapes:
/// - **Label**: leading whitespace, then `<symbol>:`. Examples:
///   `                    _start:`, `                    eval_stack:`.
/// - **Data**:  `<hex_addr>: <space-separated hex bytes>    <asm>`.
///   Example: `0004: 2A 00 01 FF    la r1, 0xFF0100`.
///
/// A label's address is the address of the next data line; if no
/// data line follows (e.g. trailing BSS reservation), the byte
/// immediately after the last data line.
#[derive(Debug, Default, Clone)]
pub struct Listing {
    symbols: BTreeMap<String, u32>,
}

enum ListingLine {
    Label(String),
    Data(u32, u32),
    Other,
}

fn classify_line(line: &str) -> ListingLine {
    let trimmed = line.trim();
    if let Some(body) = trimmed.strip_suffix(':')
        && !body.is_empty()
        && body
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && body
            .chars()
            .skip(1)
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return ListingLine::Label(body.to_string());
    }
    let trimmed_left = line.trim_start();
    if let Some((addr_part, rest)) = trimmed_left.split_once(':')
        && addr_part.chars().all(|c| c.is_ascii_hexdigit())
        && let Ok(addr) = u32::from_str_radix(addr_part, 16)
    {
        let bytes_str = rest.split("   ").next().unwrap_or(rest).trim();
        let mut count = 0u32;
        for tok in bytes_str.split_ascii_whitespace() {
            if tok.len() == 2 && tok.chars().all(|c| c.is_ascii_hexdigit()) {
                count += 1;
            } else {
                break;
            }
        }
        return ListingLine::Data(addr, count);
    }
    ListingLine::Other
}

impl Listing {
    /// Parse a listing-file string.
    pub fn parse(text: &str) -> Self {
        let mut symbols: BTreeMap<String, u32> = BTreeMap::new();
        let mut pending: Vec<String> = Vec::new();
        let mut next_after_last_data: u32 = 0;
        for raw in text.lines() {
            match classify_line(raw) {
                ListingLine::Label(name) => pending.push(name),
                ListingLine::Data(addr, byte_count) => {
                    for label in pending.drain(..) {
                        symbols.insert(label, addr);
                    }
                    next_after_last_data = addr.saturating_add(byte_count);
                }
                ListingLine::Other => {}
            }
        }
        for label in pending {
            symbols.insert(label, next_after_last_data);
        }
        Listing { symbols }
    }

    /// Resolve a symbol to its absolute address.
    pub fn resolve(&self, symbol: &str) -> Option<u32> {
        self.symbols.get(symbol).copied()
    }
}
