//! Build-tool invocation, listing parsing, and the in-process
//! cache that fronts `sw-launcher-tool`'s disk cache (`cache.rs`).
//!
//! A single `Tool` struct covers `cor24-run --assemble`, `pa24r`,
//! and `p24-load`. The kind is carried in `ToolKind`; `build()`
//! dispatches the argv accordingly. The combined struct keeps
//! `tool.rs` under sw-checklist's per-module function-count budget.
//!
//! `from_source` resolves the v1.2 schema's `[tools.<n>.source]`
//! kinds: `Path`, `FromPath { binary }`, `Sibling { path,
//! artifact }`. `Vendor` waits for a later phase.
//!
//! Phase 4 adds the disk cache: `Tool::build` first consults a
//! process-local `HashMap` (hot path), then `cache::Cache` on disk
//! (warm path between processes), then spawns the tool (cold).
//! The on-disk layout is documented in `cache.rs`.

use std::collections::{BTreeMap, HashMap};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

use crate::cache::{Cache, CacheKey};

/// Which build tool this `Tool` wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    /// `cor24-run --assemble <input> <out.bin> <out.lst>`
    Assembler,
    /// `pa24r <input> -o <out.p24>`
    Pcode,
    /// `p24-load <input.p24> -o <out.p24m> [--load-addr <addr>]`
    /// (caller passes `--load-addr` via `BuildJob.extra_args`).
    PcodeLinker,
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

/// Wraps `cor24-run --assemble`, `pa24r`, or `p24-load`. The cache
/// is keyed by `(canonical tool_path, sha256(input), argv tail)`;
/// two calls with byte-identical inputs and arguments hit the
/// process-local `HashMap` first, then the on-disk `Cache`.
#[derive(Debug)]
pub struct Tool {
    /// The host binary on disk.
    pub tool_path: Utf8PathBuf,
    /// Which tool this wraps.
    pub kind: ToolKind,
    /// Process-local cache; survives one `sw-launch` invocation.
    cache: HashMap<MemoKey, BuildOutput>,
    /// Optional on-disk cache; survives across invocations.
    disk_cache: Option<Cache>,
    /// Number of times the tool has actually been spawned by this
    /// `Tool` instance. Tests use it to verify memoization.
    pub spawn_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemoKey {
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
            disk_cache: None,
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
                    return Err(anyhow!("sibling tool not found at `{resolved}`"));
                }
                resolved
            }
        };
        Ok(Tool {
            tool_path,
            kind,
            cache: HashMap::new(),
            disk_cache: None,
            spawn_count: 0,
        })
    }

    /// Attach an on-disk cache to this tool. Subsequent `build`
    /// calls consult it after the in-process map and before
    /// spawning the tool.
    pub fn with_cache(mut self, cache: Cache) -> Self {
        self.disk_cache = Some(cache);
        self
    }

    /// Build a `job`: hot in-process cache, then warm disk cache,
    /// then spawn the tool.
    pub fn build(&mut self, job: &BuildJob) -> Result<BuildOutput> {
        let canon = self
            .tool_path
            .canonicalize_utf8()
            .unwrap_or_else(|_| self.tool_path.clone());
        let input_sha = hash_file_sha256(&job.input)?;
        let memo = MemoKey {
            tool_path: canon.to_string(),
            input_sha: input_sha.clone(),
            args: job.extra_args.clone(),
        };
        if let Some(cached) = self.cache.get(&memo) {
            return Ok(cached.clone());
        }
        let want_listing = self.kind == ToolKind::Assembler;
        let cache_key = CacheKey {
            tool_path: canon,
            input_sha,
            extra_args: job.extra_args.clone(),
            output_stem: job.output_bin.file_stem().unwrap_or("artifact").to_string(),
            output_ext: job.output_bin.extension().unwrap_or("bin").to_string(),
            with_listing: want_listing,
        };
        let out = if let Some(disk) = self.disk_cache.clone() {
            self.build_via_disk(&disk, &cache_key, job)?
        } else {
            self.build_direct(job)?
        };
        self.cache.insert(memo, out.clone());
        Ok(out)
    }

    fn build_via_disk(
        &mut self,
        disk: &Cache,
        key: &CacheKey,
        job: &BuildJob,
    ) -> Result<BuildOutput> {
        let kind = self.kind;
        let tool_path = self.tool_path.clone();
        let extra = job.extra_args.clone();
        let mut spawned = false;
        let entry = disk.get_or_fill(key, |dir| {
            let bin = dir.join(format!("{}.{}", key.output_stem, key.output_ext));
            let lst = dir.join(format!("{}.lst", key.output_stem));
            let bin_utf8 = Utf8PathBuf::from_path_buf(bin.clone())
                .map_err(|p| anyhow!("non-UTF8 cache path: {p:?}"))?;
            let lst_utf8 = Utf8PathBuf::from_path_buf(lst.clone())
                .map_err(|p| anyhow!("non-UTF8 cache path: {p:?}"))?;
            run_tool(&tool_path, kind, &job.input, &bin_utf8, &lst_utf8, &extra)?;
            spawned = true;
            Ok(())
        })?;
        if spawned {
            self.spawn_count += 1;
        }
        let bin = entry
            .dir
            .join(format!("{}.{}", key.output_stem, key.output_ext));
        let lst = if key.with_listing {
            entry.dir.join(format!("{}.lst", key.output_stem))
        } else {
            std::path::PathBuf::new()
        };
        if let Some(parent) = job.output_bin.parent() {
            std::fs::create_dir_all(parent).context("create output_bin parent")?;
        }
        std::fs::copy(&bin, job.output_bin.as_std_path())
            .with_context(|| format!("copy {} -> {}", bin.display(), job.output_bin))?;
        if key.with_listing {
            if let Some(parent) = job.output_lst.parent() {
                std::fs::create_dir_all(parent).context("create output_lst parent")?;
            }
            std::fs::copy(&lst, job.output_lst.as_std_path())
                .with_context(|| format!("copy {} -> {}", lst.display(), job.output_lst))?;
        }
        Ok(BuildOutput {
            artifact: job.output_bin.clone(),
            listing: if key.with_listing {
                job.output_lst.clone()
            } else {
                Utf8PathBuf::new()
            },
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    fn build_direct(&mut self, job: &BuildJob) -> Result<BuildOutput> {
        for p in [job.output_bin.parent(), job.output_lst.parent()]
            .into_iter()
            .flatten()
        {
            std::fs::create_dir_all(p).context("create output parent")?;
        }
        let (stdout, stderr) = run_tool(
            &self.tool_path,
            self.kind,
            &job.input,
            &job.output_bin,
            &job.output_lst,
            &job.extra_args,
        )?;
        self.spawn_count += 1;
        let listing = match self.kind {
            ToolKind::Assembler => job.output_lst.clone(),
            ToolKind::Pcode | ToolKind::PcodeLinker => Utf8PathBuf::new(),
        };
        Ok(BuildOutput {
            artifact: job.output_bin.clone(),
            listing,
            stdout,
            stderr,
        })
    }
}

fn run_tool(
    tool_path: &Utf8Path,
    kind: ToolKind,
    input: &Utf8Path,
    output_bin: &Utf8Path,
    output_lst: &Utf8Path,
    extra_args: &[String],
) -> Result<(String, String)> {
    let (in_p, out_p, lst_p) = (
        input.as_std_path(),
        output_bin.as_std_path(),
        output_lst.as_std_path(),
    );
    let mut cmd = Command::new(tool_path.as_std_path());
    match kind {
        ToolKind::Assembler => cmd.args(["--assemble".as_ref(), in_p, out_p, lst_p]),
        ToolKind::Pcode | ToolKind::PcodeLinker => cmd.args([in_p, "-o".as_ref(), out_p]),
    };
    cmd.args(extra_args);
    let output = cmd.output().context("spawn tool")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "{tool_path} exited with {}: {stderr}",
            output.status
        ));
    }
    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

/// Compute the SHA-256 of a file's contents, hex-encoded.
pub fn hash_file_sha256(path: &Utf8Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path}"))?;
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
    let paths = std::env::var_os("PATH").ok_or_else(|| anyhow!("PATH unset"))?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Utf8PathBuf::from_path_buf(candidate)
                .map_err(|p| anyhow!("non-UTF8 {binary} path: {p:?}"));
        }
    }
    Err(anyhow!("{binary} not found on PATH"))
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
