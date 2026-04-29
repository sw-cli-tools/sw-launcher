//! Build-tool invocation, per-process memoization, and listing
//! parsing.
//!
//! v1.2 (Phase 2): a single `Tool` struct covers both the COR24
//! assembler (`cor24-run --assemble`) and the p-code assembler
//! (`pa24r <input> -o <output>`). The kind is carried in
//! `ToolKind`; `build()` dispatches the argv accordingly.
//! Combining them into one struct keeps `tool.rs` under
//! sw-checklist's per-module function-count budget.
//!
//! `from_source` resolves the v1.2 schema's `[tools.<n>.source]`
//! kinds: `Path`, `FromPath { binary }`, `Sibling { path,
//! artifact }`. `Vendor` waits for Phase 4.
//!
//! The cache is in-memory and per-process: a single `sw-launch`
//! invocation that needs the same artifact twice spawns the tool
//! exactly once. Disk-persistent caching is Phase 4 work.

use std::collections::{BTreeMap, HashMap};
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Which build tool this `Tool` wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    /// `cor24-run --assemble <input> <out.bin> <out.lst>`
    Assembler,
    /// `pa24r <input> -o <out.p24>`
    Pcode,
}

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

/// How a tool's binary path is discovered. Mirrors v1.2 schema's
/// `[tools.<n>.source]` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSpec {
    /// Absolute or repo-relative path to the binary.
    Path(Utf8PathBuf),
    /// Search `$PATH` for the named binary.
    FromPath { binary: String },
    /// Relative-to-config-dir path to a sibling repo, plus the
    /// artifact path within that sibling.
    Sibling {
        path: Utf8PathBuf,
        artifact: Utf8PathBuf,
    },
}

/// Wraps either `cor24-run --assemble` or `pa24r`. The cache is
/// keyed by `(canonical tool_path, sha256(input), argv tail)`;
/// two calls with byte-identical inputs and arguments hit the
/// cache.
#[derive(Debug)]
pub struct Tool {
    /// The host binary on disk.
    pub tool_path: Utf8PathBuf,
    /// Which tool this wraps.
    pub kind: ToolKind,
    cache: HashMap<CacheKey, BuildOutput>,
    /// Number of times the tool has actually been spawned by this
    /// `Tool` instance. Tests use it to verify memoization.
    pub spawn_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    tool_path: String,
    input_sha: String,
    args: Vec<String>,
}

/// Backward-compat alias so step-007 / step-009 call sites that
/// say `Assembler::*` keep working.
pub type Assembler = Tool;

impl Tool {
    /// Construct a `Tool` wrapping the binary at `tool_path` with
    /// the given kind. Default kind for backward compat is
    /// `ToolKind::Assembler`; pass `ToolKind::Pcode` for `pa24r`.
    pub fn new(tool_path: impl Into<Utf8PathBuf>) -> Self {
        Tool {
            tool_path: tool_path.into(),
            kind: ToolKind::Assembler,
            cache: HashMap::new(),
            spawn_count: 0,
        }
    }

    /// Resolve a `SourceSpec` against the config file's directory
    /// and the tool kind, returning a constructed `Tool`.
    pub fn from_source(spec: &SourceSpec, config_dir: &Utf8Path, kind: ToolKind) -> Result<Self> {
        let tool_path = match spec {
            SourceSpec::Path(p) => {
                if p.is_absolute() {
                    p.clone()
                } else {
                    config_dir.join(p)
                }
            }
            SourceSpec::FromPath { binary } => find_binary_on_path(binary)?,
            SourceSpec::Sibling { path, artifact } => {
                let sibling_root = config_dir.join(path);
                let resolved = sibling_root.join(artifact);
                if !resolved.is_file() {
                    return Err(Error::cli(format!(
                        "sibling tool not found at `{resolved}`"
                    )));
                }
                resolved
            }
        };
        Ok(Tool {
            tool_path,
            kind,
            cache: HashMap::new(),
            spawn_count: 0,
        })
    }

    /// Build a `job`: hit the in-process cache or spawn the tool.
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
        for p in [job.output_bin.parent(), job.output_lst.parent()]
            .into_iter()
            .flatten()
        {
            std::fs::create_dir_all(p).map_err(|source| Error::Io { source })?;
        }
        let (in_p, out_p, lst_p) = (
            job.input.as_std_path(),
            job.output_bin.as_std_path(),
            job.output_lst.as_std_path(),
        );
        let mut cmd = Command::new(self.tool_path.as_std_path());
        match self.kind {
            ToolKind::Assembler => cmd.args(["--assemble".as_ref(), in_p, out_p, lst_p]),
            ToolKind::Pcode => cmd.args([in_p, "-o".as_ref(), out_p]),
        };
        cmd.args(&job.extra_args);
        let output = cmd.output().map_err(|source| Error::Io { source })?;
        self.spawn_count += 1;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::cli(format!(
                "{} exited with {}: {stderr}",
                self.tool_path, output.status
            )));
        }
        let listing = if self.kind == ToolKind::Assembler {
            job.output_lst.clone()
        } else {
            Utf8PathBuf::new()
        };
        let out = BuildOutput {
            artifact: job.output_bin.clone(),
            listing,
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

/// Search `$PATH` for `binary`. Returns the first match.
pub fn find_binary_on_path(binary: &str) -> Result<Utf8PathBuf> {
    let paths = std::env::var_os("PATH").ok_or_else(|| Error::cli("PATH unset"))?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Utf8PathBuf::from_path_buf(candidate)
                .map_err(|p| Error::cli(format!("non-UTF8 {binary} path: {p:?}")));
        }
    }
    Err(Error::cli(format!("{binary} not found on PATH")))
}

// ---------- Listing parser -----------------------------------------

/// Parsed view of a `cor24-run --assemble` `.lst` file: maps each
/// emitted symbol to its absolute address.
#[derive(Debug, Default, Clone)]
pub struct Listing {
    symbols: BTreeMap<String, u32>,
}

impl Listing {
    /// Parse a listing-file string.
    pub fn parse(text: &str) -> Self {
        let mut symbols: BTreeMap<String, u32> = BTreeMap::new();
        let mut pending: Vec<String> = Vec::new();
        let mut next_after_last_data: u32 = 0;
        for raw in text.lines() {
            let trimmed = raw.trim();
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
                pending.push(body.to_string());
                continue;
            }
            let trimmed_left = raw.trim_start();
            if let Some((addr_part, rest)) = trimmed_left.split_once(':')
                && addr_part.chars().all(|c| c.is_ascii_hexdigit())
                && let Ok(addr) = u32::from_str_radix(addr_part, 16)
            {
                let bytes_str = rest.split("   ").next().unwrap_or(rest).trim();
                let count = bytes_str
                    .split_ascii_whitespace()
                    .take_while(|t| t.len() == 2 && t.chars().all(|c| c.is_ascii_hexdigit()))
                    .count() as u32;
                for label in pending.drain(..) {
                    symbols.insert(label, addr);
                }
                next_after_last_data = addr.saturating_add(count);
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
