//! Integration tests for the `sw-launch cache list / explain /
//! clean` subcommands shipped in Phase 4 step 2.
//!
//! Each test pre-seeds a tempdir cache with synthetic entries so
//! the assertions are deterministic and don't depend on
//! cor24-run / pa24r being on PATH. The cache subcommands only
//! need the on-disk layout the prior step wrote, not the live
//! tools that produce it.

use std::path::Path;

use assert_cmd::Command;
use camino::Utf8PathBuf;
use predicates::str::contains;
use sw_launcher_tool::cache::{Cache, CacheKey};

fn cmd() -> Command {
    Command::cargo_bin("sw-launch").expect("binary built")
}

fn seed_entry(cache: &Cache, stem: &str, layer: Option<&str>, when_unix: Option<u64>) {
    let key = CacheKey {
        tool_path: Utf8PathBuf::from(format!("/usr/bin/fake-{stem}")),
        input_sha: format!("{stem}-input-sha-{:0>56}", "0"),
        extra_args: vec![],
        output_stem: stem.to_string(),
        output_ext: "bin".into(),
        with_listing: false,
    };
    cache
        .get_or_fill_with(&key, layer, |dir| {
            std::fs::write(dir.join(format!("{stem}.bin")), stem.as_bytes())?;
            Ok(())
        })
        .expect("seed");
    if let Some(t) = when_unix {
        backdate(cache.list().unwrap()[0].dir.as_path(), t);
    }
}

fn backdate(dir: &Path, unix: u64) {
    let prov = dir.join("provenance.toml");
    let mut text = std::fs::read_to_string(&prov).expect("read prov");
    text = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("created_unix") {
                format!("created_unix = {unix}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&prov, text).expect("write prov");
}

#[test]
fn cache_list_prints_seeded_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    seed_entry(&cache, "alpha", Some("pcode_vm"), None);
    seed_entry(&cache, "beta", Some("ocaml_interp"), None);

    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "list"])
        .assert()
        .success()
        .stdout(contains("DIGEST"))
        .stdout(contains("pcode_vm"))
        .stdout(contains("ocaml_interp"));
}

#[test]
fn cache_list_json_emits_array() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    seed_entry(&cache, "json", Some("layer_x"), None);

    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "list", "--json"])
        .assert()
        .success()
        .stdout(contains("\"layer\":\"layer_x\""))
        .stdout(contains("\"digest\""));
}

#[test]
fn cache_explain_prints_full_provenance_for_unique_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    seed_entry(&cache, "explainme", Some("the_layer"), None);
    let digest = cache.list().unwrap()[0].digest.clone();
    let prefix = &digest[..8];

    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "explain", prefix])
        .assert()
        .success()
        .stdout(contains("the_layer"))
        .stdout(contains("artifact_sha:"))
        .stdout(contains("created_unix:"));
}

#[test]
fn cache_explain_with_ambiguous_prefix_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    seed_entry(&cache, "amb_a", None, None);
    seed_entry(&cache, "amb_b", None, None);
    // Empty prefix matches every entry.
    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "explain", ""])
        .assert()
        .failure()
        .stderr(contains("ambiguous"));
}

#[test]
fn cache_clean_dry_run_is_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    seed_entry(&cache, "stale", None, Some(0)); // very old
    let before = cache.list().unwrap().len();

    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "clean", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would remove"));

    let after = Cache::open_at(tmp.path().to_path_buf())
        .unwrap()
        .list()
        .unwrap()
        .len();
    assert_eq!(before, after, "dry-run must not delete entries");
}

#[test]
fn cache_clean_older_than_removes_only_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    seed_entry(&cache, "old", None, Some(now.saturating_sub(86_400 * 60))); // 60d old
    seed_entry(&cache, "fresh", None, None);

    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "clean", "--older-than", "30d"])
        .assert()
        .success();

    let remaining = Cache::open_at(tmp.path().to_path_buf())
        .unwrap()
        .list()
        .unwrap();
    assert_eq!(remaining.len(), 1, "exactly the fresh entry should remain");
}

#[test]
fn cache_clean_all_wipes_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    seed_entry(&cache, "one", None, None);
    seed_entry(&cache, "two", None, None);

    cmd()
        .env("SW_LAUNCH_CACHE_DIR", tmp.path())
        .args(["cache", "clean", "--all"])
        .assert()
        .success()
        .stdout(contains("removed"));

    let remaining = Cache::open_at(tmp.path().to_path_buf())
        .unwrap()
        .list()
        .unwrap();
    assert!(remaining.is_empty(), "everything should be gone");
}
