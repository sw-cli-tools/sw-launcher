//! End-to-end test for Scenario A: assemble echo.s, run it under
//! the real cor24-run, assert UART contains "A".
//!
//! Skipped (with a one-line note on stderr) if cor24-run is not on
//! PATH; this keeps cargo test green on machines that don't have
//! the emulator installed.

use std::path::PathBuf;

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

#[test]
fn sw_launch_run_echo_against_real_cor24_run() {
    if !cor24_run_on_path() {
        eprintln!("cor24-run not on PATH; skipping Scenario A end-to-end test");
        return;
    }
    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .args([
            "run",
            "echo",
            "--config",
            fixture_dir().join("sw-launch.toml").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("A"));
}

#[test]
fn run_with_failing_expectation_exits_nonzero_and_prints_mismatch() {
    if !cor24_run_on_path() {
        eprintln!("cor24-run not on PATH; skipping mismatch test");
        return;
    }
    let dir = fixture_dir();
    let bad_toml = dir.join("bad-expect.toml");
    let original = std::fs::read_to_string(dir.join("sw-launch.toml")).unwrap();
    let bad = original.replace(r#"uart_contains = ["A"]"#, r#"uart_contains = ["WRONG"]"#);
    std::fs::write(&bad_toml, bad).unwrap();
    let _cleanup = scopeguard(|| {
        let _ = std::fs::remove_file(&bad_toml);
    });

    Command::cargo_bin("sw-launch")
        .expect("binary built")
        .args(["run", "echo", "--config", bad_toml.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("expectation mismatch"))
        .stderr(contains("WRONG"));
}

/// Tiny RAII helper so the bad-toml fixture cleans up even if the
/// test panics. Keeps tests/scenario_a.rs free of an external
/// scopeguard dep (and respects sw-checklist's module-count cap).
struct Scope<F: FnOnce()>(Option<F>);
impl<F: FnOnce()> Drop for Scope<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}
fn scopeguard<F: FnOnce()>(f: F) -> Scope<F> {
    Scope(Some(f))
}
