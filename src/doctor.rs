//! `sw-launch doctor`: host-environment + manifest pre-flight.
//!
//! Phase 4 step 4. Single command an agent or user runs to find
//! out whether `sw-launch run` would succeed against a given
//! manifest, *before* hitting cor24-run-not-on-PATH or
//! ocaml.p24m-missing failures partway through.
//!
//! Without `--config`: only host-tool checks (cor24-run, pa24r,
//! p24-load, cache dir writable).
//!
//! With `--config`: also verifies every `layer.input` and
//! `sidecar:<path>` resolves on disk, and reads
//! `sw-launch.lock` (when present) to surface drift.

use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use sw_launcher_lockfile::{EntryStatus, Lockfile};
use sw_launcher_tool::cache::Cache;
use sw_launcher_tool::tool::find_binary_on_path;

use crate::config::Config;
use crate::error::{Error, Result};

/// One row in the doctor's report.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
}

/// Severity for a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Warn,
    Fail,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        }
    }
}

/// Run the full check suite. Returns Err only on internal IO
/// errors -- regular check failures surface in the printed
/// report and the exit code.
pub fn run(config: Option<&Utf8Path>, json: bool) -> Result<()> {
    let mut results: Vec<CheckResult> = vec![
        check_tool_with_version("cor24-run", true),
        check_tool_on_path("pa24r"),
        check_tool_on_path("p24-load"),
        check_cache_writable(),
    ];
    if let Some(cfg_path) = config {
        match Config::from_path(cfg_path) {
            Ok(cfg) => {
                let cfg_dir = cfg_path.parent().unwrap_or_else(|| Utf8Path::new("."));
                results.extend(check_manifest_inputs(&cfg, cfg_dir));
                results.push(check_lockfile(cfg_dir));
            }
            Err(e) => results.push(CheckResult {
                name: "manifest",
                status: Status::Fail,
                detail: format!("{cfg_path}: {e}"),
            }),
        }
    }
    if json {
        println!("{}", format_json(&results));
    } else {
        print_table(&results);
    }
    if results.iter().any(|r| r.status == Status::Fail) {
        return Err(Error::cli("doctor: one or more checks failed".to_string()));
    }
    Ok(())
}

fn print_table(results: &[CheckResult]) {
    for r in results {
        println!("[{}]  {:<20}  {}", r.status.label(), r.name, r.detail);
    }
}

fn format_json(results: &[CheckResult]) -> String {
    let mut out = String::from("[");
    for (i, r) in results.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let detail = r.detail.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"status\":\"{}\",\"detail\":\"{detail}\"}}",
            r.name,
            r.status.label()
        ));
    }
    out.push(']');
    out
}

fn check_tool_with_version(binary: &'static str, required: bool) -> CheckResult {
    match find_binary_on_path(binary) {
        Ok(path) => match Command::new(path.as_std_path()).arg("--version").output() {
            Ok(out) if out.status.success() => CheckResult {
                name: binary,
                status: Status::Pass,
                detail: String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
            },
            Ok(out) => CheckResult {
                name: binary,
                status: Status::Fail,
                detail: format!(
                    "exited {}: {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            },
            Err(e) => CheckResult {
                name: binary,
                status: Status::Fail,
                detail: format!("spawn failed: {e}"),
            },
        },
        Err(_) => CheckResult {
            name: binary,
            status: if required { Status::Fail } else { Status::Warn },
            detail: format!("`{binary}` not on PATH"),
        },
    }
}

fn check_tool_on_path(binary: &'static str) -> CheckResult {
    match find_binary_on_path(binary) {
        Ok(path) => CheckResult {
            name: binary,
            status: Status::Pass,
            detail: path.to_string(),
        },
        Err(_) => CheckResult {
            name: binary,
            status: Status::Warn,
            detail: format!("`{binary}` not on PATH (only needed for pcode layers)"),
        },
    }
}

fn check_cache_writable() -> CheckResult {
    match Cache::open_default() {
        Err(e) => CheckResult {
            name: "cache",
            status: Status::Fail,
            detail: format!("{e:#}"),
        },
        Ok(_) => {
            // Open succeeded -> the directory tree was creatable.
            // Confirm with a probe write at the resolved root.
            let root = std::env::var_os("SW_LAUNCH_CACHE_DIR")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("XDG_CACHE_HOME")
                        .map(|x| std::path::PathBuf::from(x).join("sw-launch"))
                })
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(|h| std::path::PathBuf::from(h).join(".cache").join("sw-launch"))
                });
            let Some(root) = root else {
                return CheckResult {
                    name: "cache",
                    status: Status::Fail,
                    detail: "HOME unset; cannot resolve cache dir".into(),
                };
            };
            let probe = root.join(".doctor-probe");
            match std::fs::write(&probe, b"ok") {
                Ok(_) => {
                    let _ = std::fs::remove_file(&probe);
                    CheckResult {
                        name: "cache",
                        status: Status::Pass,
                        detail: format!("writable at {}", root.display()),
                    }
                }
                Err(e) => CheckResult {
                    name: "cache",
                    status: Status::Fail,
                    detail: format!("{}: {e}", probe.display()),
                },
            }
        }
    }
}

fn check_manifest_inputs(cfg: &Config, cfg_dir: &Utf8Path) -> Vec<CheckResult> {
    let mut out = Vec::new();
    for (name, layer) in &cfg.layers {
        if let Some(input) = layer.input.as_deref() {
            let resolved: Utf8PathBuf = if Utf8Path::new(input).is_absolute() {
                Utf8PathBuf::from(input)
            } else {
                cfg_dir.join(input)
            };
            out.push(if resolved.as_std_path().is_file() {
                CheckResult {
                    name: "layer.input",
                    status: Status::Pass,
                    detail: format!("{name}: {resolved}"),
                }
            } else {
                CheckResult {
                    name: "layer.input",
                    status: Status::Fail,
                    detail: format!("{name}: missing `{resolved}`"),
                }
            });
        }
        for p in &layer.patches {
            for term in [&p.target, &p.value] {
                if let Some(rel) = term.strip_prefix("sidecar:") {
                    let resolved: Utf8PathBuf = if Utf8Path::new(rel).is_absolute() {
                        Utf8PathBuf::from(rel)
                    } else {
                        cfg_dir.join(rel)
                    };
                    out.push(if resolved.as_std_path().is_file() {
                        CheckResult {
                            name: "sidecar",
                            status: Status::Pass,
                            detail: resolved.to_string(),
                        }
                    } else {
                        CheckResult {
                            name: "sidecar",
                            status: Status::Fail,
                            detail: format!("missing `{resolved}`"),
                        }
                    });
                }
            }
        }
    }
    out
}

fn check_lockfile(cfg_dir: &Utf8Path) -> CheckResult {
    let path = cfg_dir.join("sw-launch.lock");
    match Lockfile::read_optional(path.as_std_path()) {
        Ok(Some(lock)) => {
            let mut drifted = 0usize;
            let mut unresolvable = 0usize;
            for (_, status) in lock.status() {
                match status {
                    EntryStatus::Fresh => {}
                    EntryStatus::Drifted { .. } | EntryStatus::Unverified(_) => drifted += 1,
                    EntryStatus::Unresolvable => unresolvable += 1,
                }
            }
            if drifted == 0 && unresolvable == 0 {
                CheckResult {
                    name: "lockfile",
                    status: Status::Pass,
                    detail: format!("{} entries fresh", lock.vendored.len()),
                }
            } else {
                CheckResult {
                    name: "lockfile",
                    status: Status::Fail,
                    detail: format!(
                        "E0041 {drifted} drifted, E0042 {unresolvable} unresolvable (run `sw-launch vendor sync`)"
                    ),
                }
            }
        }
        Ok(None) => CheckResult {
            name: "lockfile",
            status: Status::Warn,
            detail: format!("no sw-launch.lock at {path} (run `sw-launch vendor sync` to pin)"),
        },
        Err(e) => CheckResult {
            name: "lockfile",
            status: Status::Fail,
            detail: format!("{e}"),
        },
    }
}
