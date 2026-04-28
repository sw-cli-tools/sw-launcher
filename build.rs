//! Build script that captures version-adjacent metadata for the
//! `sw-launch --version` output (commit, host, build time).
//!
//! Required by sw-checklist's "Version Field: ..." rules.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let commit =
        run_git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let host = host_name().unwrap_or_else(|| "unknown".to_string());
    let build_time = build_time_iso8601();

    println!("cargo:rustc-env=SW_LAUNCH_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=SW_LAUNCH_BUILD_HOST={host}");
    println!("cargo:rustc-env=SW_LAUNCH_BUILD_TIME={build_time}");

    // Re-run when HEAD moves; not strictly required since cargo
    // already invalidates on Cargo.toml changes, but keeps the
    // commit field fresh during local iteration.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=build.rs");
}

fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    Some(s.trim().to_string())
}

fn host_name() -> Option<String> {
    // POSIX `hostname` is universally available; std doesn't expose it.
    let output = Command::new("hostname").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    Some(s.trim().to_string())
}

fn build_time_iso8601() -> String {
    // Minimal UTC formatter: avoids a chrono dependency for one
    // string. Format: YYYY-MM-DDTHH:MM:SSZ.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_utc(now as i64)
}

fn format_unix_utc(secs: i64) -> String {
    let (y, mo, d, h, mi, s) = unix_to_ymdhms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Convert a Unix timestamp to UTC (year, month, day, hour, minute, second).
///
/// Civil-from-days algorithm by Howard Hinnant; portable to any
/// proleptic Gregorian date in i64 range.
fn unix_to_ymdhms(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let h = (tod / 3600) as u32;
    let mi = ((tod / 60) % 60) as u32;
    let s = (tod % 60) as u32;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (y, m, d, h, mi, s)
}
