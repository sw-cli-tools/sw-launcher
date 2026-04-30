//! Content-addressed disk cache for sw-launcher build artifacts.
//!
//! Phase 4 step 1 promotes the in-process `HashMap` cache to a
//! disk-persistent store under
//! `${XDG_CACHE_HOME:-$HOME/.cache}/sw-launch/`:
//!
//! ```text
//! artifacts/<digest>/
//!   <stem>.<ext>           # the artifact bytes
//!   <stem>.lst             # listing (Assembler kind only)
//!   provenance.toml        # cache key fields + timestamp + host
//! locks/<digest>.lock      # advisory file lock for serialization
//! ```
//!
//! `Cache::get_or_fill` is the entry point: it acquires an
//! exclusive lock on `locks/<digest>.lock`, checks for a fresh
//! entry under `artifacts/<digest>/`, and on miss runs a
//! caller-supplied `fill` closure into a temp directory before
//! atomically renaming it into place.
//!
//! Verification on read: `provenance.toml` records the SHA-256 of
//! every artifact byte the closure produced; reads re-hash the
//! files and re-fill if they don't match (defends against partial
//! writes that survived a crash, or manual edits to the cache
//! directory).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One cache lookup. The `digest()` method canonicalizes the key
/// to a hex sha256 used as the on-disk directory name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    pub tool_path: Utf8PathBuf,
    pub input_sha: String,
    pub extra_args: Vec<String>,
    pub output_stem: String,
    pub output_ext: String,
    /// Whether the tool also emits a `.lst` listing alongside the
    /// main artifact. Affects which files the cache verifies.
    pub with_listing: bool,
}

impl CacheKey {
    /// SHA-256 over a canonical encoding of every key field.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.tool_path.as_str().as_bytes());
        hasher.update(b"\x00");
        hasher.update(self.input_sha.as_bytes());
        hasher.update(b"\x00");
        for a in &self.extra_args {
            hasher.update(a.as_bytes());
            hasher.update(b"\x00");
        }
        hasher.update(b"\x00");
        hasher.update(self.output_stem.as_bytes());
        hasher.update(b"\x00");
        hasher.update(self.output_ext.as_bytes());
        hasher.update(b"\x00");
        hasher.update(if self.with_listing { b"L" } else { b"-" });
        let d = hasher.finalize();
        let mut hex = String::with_capacity(d.len() * 2);
        for b in d {
            use std::fmt::Write;
            let _ = write!(hex, "{b:02x}");
        }
        hex
    }
}

/// One on-disk entry: the directory holding the cached files and
/// the deserialized provenance record alongside.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub dir: PathBuf,
    pub provenance: Provenance,
}

/// Metadata recorded next to the cached artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub schema_version: u32,
    pub tool_path: String,
    pub input_sha256: String,
    pub extra_args: Vec<String>,
    pub output_stem: String,
    pub output_ext: String,
    pub with_listing: bool,
    pub artifact_sha256: String,
    pub listing_sha256: Option<String>,
    pub created_unix: u64,
    pub host: String,
}

/// Disk cache rooted at a directory.
#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// Open the default cache at `${XDG_CACHE_HOME:-$HOME/.cache}/sw-launch/`,
    /// honoring `SW_LAUNCH_CACHE_DIR` as an override.
    pub fn open_default() -> Result<Self> {
        let root = if let Some(p) = std::env::var_os("SW_LAUNCH_CACHE_DIR") {
            PathBuf::from(p)
        } else if let Some(p) = std::env::var_os("XDG_CACHE_HOME") {
            PathBuf::from(p).join("sw-launch")
        } else {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow!("HOME unset; cannot resolve cache dir"))?;
            PathBuf::from(home).join(".cache").join("sw-launch")
        };
        Self::open_at(root)
    }

    /// Open a cache rooted at `root`. Creates the directory tree
    /// if needed.
    pub fn open_at(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(root.join("artifacts"))
            .with_context(|| format!("create {}/artifacts", root.display()))?;
        std::fs::create_dir_all(root.join("locks"))
            .with_context(|| format!("create {}/locks", root.display()))?;
        Ok(Cache { root })
    }

    /// Look up `key`. On hit, return the entry. On miss (or
    /// corruption), acquire an exclusive lock, run `fill`, atomically
    /// commit the result, and return the entry.
    ///
    /// `fill` writes its output files (`<stem>.<ext>` and optionally
    /// `<stem>.lst`) into the directory passed to it. The cache
    /// verifies their presence and records SHA-256 sums in
    /// `provenance.toml`.
    pub fn get_or_fill<F>(&self, key: &CacheKey, fill: F) -> Result<CacheEntry>
    where
        F: FnOnce(&Path) -> Result<()>,
    {
        let digest = key.digest();
        let final_dir = self.root.join("artifacts").join(&digest);
        let lock_path = self.root.join("locks").join(format!("{digest}.lock"));
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("open lock {}", lock_path.display()))?;
        lock.lock()
            .with_context(|| format!("flock {}", lock_path.display()))?;
        if let Some(entry) = self.read_if_fresh(key, &final_dir)? {
            return Ok(entry);
        }
        let temp_dir = self.root.join("artifacts").join(format!(".tmp-{digest}"));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir)
            .with_context(|| format!("create temp {}", temp_dir.display()))?;
        fill(&temp_dir).context("cache fill closure")?;
        let prov = self.write_provenance(key, &temp_dir)?;
        let _ = std::fs::remove_dir_all(&final_dir);
        std::fs::rename(&temp_dir, &final_dir)
            .with_context(|| format!("rename {} -> {}", temp_dir.display(), final_dir.display()))?;
        Ok(CacheEntry {
            dir: final_dir,
            provenance: prov,
        })
    }

    fn read_if_fresh(&self, key: &CacheKey, dir: &Path) -> Result<Option<CacheEntry>> {
        let prov_path = dir.join("provenance.toml");
        if !prov_path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&prov_path)
            .with_context(|| format!("read {}", prov_path.display()))?;
        let prov: Provenance = match toml::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        let bin_path = dir.join(format!("{}.{}", key.output_stem, key.output_ext));
        if !bin_path.is_file() {
            return Ok(None);
        }
        let bin_bytes =
            std::fs::read(&bin_path).with_context(|| format!("read {}", bin_path.display()))?;
        if hash_bytes(&bin_bytes) != prov.artifact_sha256 {
            return Ok(None);
        }
        if key.with_listing {
            let lst_path = dir.join(format!("{}.lst", key.output_stem));
            if !lst_path.is_file() {
                return Ok(None);
            }
            let lst_bytes =
                std::fs::read(&lst_path).with_context(|| format!("read {}", lst_path.display()))?;
            match prov.listing_sha256.as_deref() {
                Some(sha) if sha == hash_bytes(&lst_bytes) => {}
                _ => return Ok(None),
            }
        }
        Ok(Some(CacheEntry {
            dir: dir.to_path_buf(),
            provenance: prov,
        }))
    }

    fn write_provenance(&self, key: &CacheKey, dir: &Path) -> Result<Provenance> {
        let bin_path = dir.join(format!("{}.{}", key.output_stem, key.output_ext));
        let bin_bytes = std::fs::read(&bin_path)
            .with_context(|| format!("read filled {}", bin_path.display()))?;
        let artifact_sha = hash_bytes(&bin_bytes);
        let listing_sha = if key.with_listing {
            let lst_path = dir.join(format!("{}.lst", key.output_stem));
            let bytes = std::fs::read(&lst_path)
                .with_context(|| format!("read filled {}", lst_path.display()))?;
            Some(hash_bytes(&bytes))
        } else {
            None
        };
        let prov = Provenance {
            schema_version: 1,
            tool_path: key.tool_path.to_string(),
            input_sha256: key.input_sha.clone(),
            extra_args: key.extra_args.clone(),
            output_stem: key.output_stem.clone(),
            output_ext: key.output_ext.clone(),
            with_listing: key.with_listing,
            artifact_sha256: artifact_sha,
            listing_sha256: listing_sha,
            created_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            host: std::env::var("HOSTNAME")
                .ok()
                .or_else(|| std::env::var("HOST").ok())
                .unwrap_or_else(|| "unknown".to_string()),
        };
        let toml_text = toml::to_string_pretty(&prov).context("serialize provenance")?;
        let prov_path = dir.join("provenance.toml");
        let mut f =
            File::create(&prov_path).with_context(|| format!("create {}", prov_path.display()))?;
        f.write_all(toml_text.as_bytes())?;
        Ok(prov)
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let d = hasher.finalize();
    let mut hex = String::with_capacity(d.len() * 2);
    for b in d {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}
