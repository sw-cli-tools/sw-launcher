//! End-to-end test for the disk cache.
//!
//! Phase 4 step 1 acceptance: a second `sw-launch run` against the
//! same fixture must hit the disk cache rather than re-assembling.
//! Verified at the cache-directory level: after two runs the
//! `artifacts/` directory contains exactly one entry per
//! distinct cache key (i.e. one for the assembled image), because
//! identical inputs produce identical digests; a missed cache
//! would land in the same directory rather than create a new one,
//! but its provenance.toml's `created_unix` would differ.

use std::path::{Path, PathBuf};
use std::time::Duration;

use assert_cmd::Command;
use predicates::str::contains;

fn cor24_run_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join("cor24-run").is_file()))
        .unwrap_or(false)
}

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/scenario_a");
    p
}

fn read_provenance_created_unix(cache_root: &Path) -> Option<u64> {
    let artifacts = cache_root.join("artifacts");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&artifacts)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| !n.starts_with(".tmp"))
        })
        .collect();
    entries.sort();
    let prov_path = entries.first()?.join("provenance.toml");
    let text = std::fs::read_to_string(prov_path).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("created_unix") {
            let val = rest.trim_start_matches([' ', '=']).trim();
            return val.parse().ok();
        }
    }
    None
}

#[test]
fn second_run_hits_cache_for_assembled_image() {
    if !cor24_run_on_path() {
        eprintln!("cor24-run not on PATH; skipping disk-cache persistence test");
        return;
    }
    let cache_dir = tempfile::tempdir().expect("tempdir");
    let cache_root = cache_dir.path().to_path_buf();
    let toml = fixture_dir().join("sw-launch.toml");

    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .env("SW_LAUNCH_CACHE_DIR", &cache_root)
        .args(["run", "echo", "--config", toml.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("A"));

    let first_created =
        read_provenance_created_unix(&cache_root).expect("first run should populate the cache");

    // Sleep just long enough that a second cache fill (if it
    // happened) would record a strictly later created_unix.
    std::thread::sleep(Duration::from_millis(1100));

    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .env("SW_LAUNCH_CACHE_DIR", &cache_root)
        .args(["run", "echo", "--config", toml.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("A"));

    let second_created = read_provenance_created_unix(&cache_root)
        .expect("second run should still see the cache entry");
    assert_eq!(
        first_created, second_created,
        "provenance.toml created_unix must not change on cache hit"
    );

    let entries: Vec<_> = std::fs::read_dir(cache_root.join("artifacts"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| !n.starts_with(".tmp"))
        })
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "exactly one cache entry expected; saw {}",
        entries.len()
    );
}
