//! `sw-launch.lock` writer / reader and drift detection.
//!
//! Phase 4 step 3 lands the lockfile so sibling-repo
//! dependencies (vendored COR24 binaries, the OCaml runtime,
//! the p-code VM source) are pinned across machines and CI.
//!
//! Layout (one [[vendored]] table per distinct input across all
//! scenarios, sorted by `key`):
//!
//! ```toml
//! schema_version = 1
//!
//! [[vendored]]
//! key           = "sw-cor24-pcode/vm/pvm.s"
//! resolved_path = "/abs/path/at/sync/time/pvm.s"
//! vendor_repo   = "sw-embed/sw-cor24-pcode"      # if known
//! vendor_sha    = "abc1234..."                    # if .git
//! input_sha256  = "..."                           # 64 hex
//! synced_at     = "2026-04-30T12:00:00Z"
//! ```
//!
//! `vendor sync` writes the file. `vendor status` reads it and
//! reports drift. `sw-launch run`/`build` consult it via
//! `Lockfile::status` and refuse to proceed on stale entries
//! (E0041) unless `--update-lock` is passed.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sw_launcher_tool::tool::hash_file_sha256;

/// The on-disk lockfile, deserialized.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lockfile {
    pub schema_version: u32,
    #[serde(default)]
    pub vendored: Vec<VendoredEntry>,
}

/// One pinned input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VendoredEntry {
    pub key: String,
    pub resolved_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_sha: Option<String>,
    pub input_sha256: String,
    pub synced_at: String,
}

/// Per-entry comparison result returned by [`Lockfile::status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryStatus {
    /// On-disk sha matches the lockfile.
    Fresh,
    /// File present but sha mismatch.
    Drifted { recorded: String, observed: String },
    /// `resolved_path` doesn't exist any more (E0042).
    Unresolvable,
    /// File present but couldn't be read (permission, etc).
    Unverified(String),
}

impl Default for Lockfile {
    fn default() -> Self {
        Lockfile {
            schema_version: 1,
            vendored: Vec::new(),
        }
    }
}

impl Lockfile {
    /// Read a lockfile from `path`.
    pub fn read(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let lf: Lockfile =
            toml::from_str(&text).with_context(|| format!("parse {} as TOML", path.display()))?;
        Ok(lf)
    }

    /// Read a lockfile if it exists; otherwise return None.
    pub fn read_optional(path: &Path) -> Result<Option<Self>> {
        if !path.is_file() {
            return Ok(None);
        }
        Self::read(path).map(Some)
    }

    /// Serialize and write atomically (temp file + rename).
    pub fn write(&self, path: &Path) -> Result<()> {
        let text = self.to_toml_string();
        let tmp = path.with_extension("lock.tmp");
        std::fs::write(&tmp, &text).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }

    /// Render the lockfile as a stable canonical TOML string.
    /// Vendored entries are emitted sorted by `key`.
    pub fn to_toml_string(&self) -> String {
        let mut sorted = self.vendored.clone();
        sorted.sort_by(|a, b| a.key.cmp(&b.key));
        let canonical = Lockfile {
            schema_version: self.schema_version,
            vendored: sorted,
        };
        toml::to_string_pretty(&canonical).expect("Lockfile is always TOML-serializable")
    }

    /// Compare every recorded entry against the file at its
    /// `resolved_path`. Returns one [`EntryStatus`] per entry, in
    /// declaration order.
    pub fn status(&self) -> Vec<(String, EntryStatus)> {
        self.vendored
            .iter()
            .map(|e| (e.key.clone(), check_entry(e)))
            .collect()
    }
}

fn check_entry(e: &VendoredEntry) -> EntryStatus {
    let path = Path::new(&e.resolved_path);
    if !path.is_file() {
        return EntryStatus::Unresolvable;
    }
    let utf8 = match Utf8Path::from_path(path) {
        Some(p) => p,
        None => return EntryStatus::Unverified("non-UTF8 path".into()),
    };
    match hash_file_sha256(utf8) {
        Ok(observed) => {
            if observed == e.input_sha256 {
                EntryStatus::Fresh
            } else {
                EntryStatus::Drifted {
                    recorded: e.input_sha256.clone(),
                    observed,
                }
            }
        }
        Err(err) => EntryStatus::Unverified(format!("{err:#}")),
    }
}

/// One input the lockfile should record. Callers (`vendor sync`)
/// build a `Vec<VendorInput>` by walking the manifest's layers
/// and tools.
#[derive(Debug, Clone)]
pub struct VendorInput {
    /// Stable, repo-relative key. For sibling-repo files,
    /// `<sibling-relpath>/<artifact>`. For FromPath tool binaries,
    /// the binary name.
    pub key: String,
    /// Absolute on-disk path resolved at sync time.
    pub resolved_path: Utf8PathBuf,
    /// Owning repo identifier ("org/repo"), if discoverable.
    pub vendor_repo: Option<String>,
}

/// Build a fresh `Lockfile` by hashing every `VendorInput`.
/// `now_iso8601` is supplied by the caller so tests can pin it.
pub fn sync(inputs: &[VendorInput], now_iso8601: &str) -> Result<Lockfile> {
    let mut by_key: BTreeMap<String, VendoredEntry> = BTreeMap::new();
    for inp in inputs {
        if !inp.resolved_path.as_std_path().is_file() {
            return Err(anyhow!(
                "vendor input `{}` does not resolve to a file at `{}`",
                inp.key,
                inp.resolved_path
            ));
        }
        let sha = hash_file_sha256(&inp.resolved_path)?;
        let entry = VendoredEntry {
            key: inp.key.clone(),
            resolved_path: inp.resolved_path.to_string(),
            vendor_repo: inp.vendor_repo.clone(),
            vendor_sha: read_git_head(&inp.resolved_path),
            input_sha256: sha,
            synced_at: now_iso8601.to_string(),
        };
        by_key.insert(inp.key.clone(), entry);
    }
    Ok(Lockfile {
        schema_version: 1,
        vendored: by_key.into_values().collect(),
    })
}

/// Like [`sync`] but only rewrites entries that drifted from
/// `prior`. Fresh entries keep their original `synced_at`.
pub fn update_only_drifted(
    prior: &Lockfile,
    inputs: &[VendorInput],
    now_iso8601: &str,
) -> Result<Lockfile> {
    let prior_by_key: BTreeMap<&str, &VendoredEntry> =
        prior.vendored.iter().map(|e| (e.key.as_str(), e)).collect();
    let mut out: BTreeMap<String, VendoredEntry> = BTreeMap::new();
    for inp in inputs {
        if !inp.resolved_path.as_std_path().is_file() {
            return Err(anyhow!(
                "vendor input `{}` does not resolve to a file at `{}`",
                inp.key,
                inp.resolved_path
            ));
        }
        let sha = hash_file_sha256(&inp.resolved_path)?;
        let entry = match prior_by_key.get(inp.key.as_str()) {
            Some(existing) if existing.input_sha256 == sha => (*existing).clone(),
            _ => VendoredEntry {
                key: inp.key.clone(),
                resolved_path: inp.resolved_path.to_string(),
                vendor_repo: inp.vendor_repo.clone(),
                vendor_sha: read_git_head(&inp.resolved_path),
                input_sha256: sha,
                synced_at: now_iso8601.to_string(),
            },
        };
        out.insert(inp.key.clone(), entry);
    }
    Ok(Lockfile {
        schema_version: 1,
        vendored: out.into_values().collect(),
    })
}

fn read_git_head(file: &Utf8Path) -> Option<String> {
    let mut dir = file.parent()?.to_path_buf();
    loop {
        let head = dir.join(".git").join("HEAD");
        if head.is_file() {
            let text = std::fs::read_to_string(head.as_std_path()).ok()?;
            let trimmed = text.trim();
            if let Some(refname) = trimmed.strip_prefix("ref: ") {
                let ref_path = dir.join(".git").join(refname);
                if let Ok(sha) = std::fs::read_to_string(ref_path.as_std_path()) {
                    return Some(sha.trim().to_string());
                }
                return None;
            }
            return Some(trimmed.to_string());
        }
        if !dir.pop() {
            return None;
        }
    }
}
