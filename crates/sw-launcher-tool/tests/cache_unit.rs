//! Unit tests for `sw_launcher_tool::cache`.
//!
//! Phase 4 step 1: cover the four behaviours called out in the
//! step plan -- hit, miss-runs-fill-once, corrupt-refilled, and
//! concurrent-fills-serialized -- so the disk cache contract is
//! locked in before subsequent steps add subcommands and the
//! lockfile pile on top.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use camino::Utf8PathBuf;
use sw_launcher_tool::cache::{Cache, CacheKey};
use tempfile::TempDir;

fn fresh_key(stem: &str) -> CacheKey {
    CacheKey {
        tool_path: Utf8PathBuf::from("/usr/bin/fake-tool"),
        input_sha: "deadbeef".repeat(8),
        extra_args: vec!["--load-addr".into(), "0x010000".into()],
        output_stem: stem.to_string(),
        output_ext: "bin".into(),
        with_listing: false,
    }
}

#[test]
fn cache_hit_returns_same_path_without_invoking_fill() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    let key = fresh_key("hello");
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_a = Arc::clone(&calls);
    let entry_a = cache
        .get_or_fill(&key, |dir| {
            calls_a.fetch_add(1, Ordering::SeqCst);
            std::fs::write(dir.join("hello.bin"), b"AAA")?;
            Ok(())
        })
        .unwrap();
    let calls_b = Arc::clone(&calls);
    let entry_b = cache
        .get_or_fill(&key, |dir| {
            calls_b.fetch_add(1, Ordering::SeqCst);
            std::fs::write(dir.join("hello.bin"), b"BBB")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(entry_a.dir, entry_b.dir);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "fill ran exactly once");
    let bytes = std::fs::read(entry_b.dir.join("hello.bin")).unwrap();
    assert_eq!(&bytes, b"AAA", "second call returns the first call's bytes");
}

#[test]
fn corrupt_artifact_is_refilled() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    let key = fresh_key("corrupt");
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_a = Arc::clone(&calls);
    let first = cache
        .get_or_fill(&key, |dir| {
            calls_a.fetch_add(1, Ordering::SeqCst);
            std::fs::write(dir.join("corrupt.bin"), b"original")?;
            Ok(())
        })
        .unwrap();
    // simulate manual edit / partial write surviving a crash
    std::fs::write(first.dir.join("corrupt.bin"), b"tampered").unwrap();
    let calls_b = Arc::clone(&calls);
    let second = cache
        .get_or_fill(&key, |dir| {
            calls_b.fetch_add(1, Ordering::SeqCst);
            std::fs::write(dir.join("corrupt.bin"), b"refilled")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "fill ran twice (re-fill on corruption)"
    );
    let bytes = std::fs::read(second.dir.join("corrupt.bin")).unwrap();
    assert_eq!(&bytes, b"refilled");
}

#[test]
fn concurrent_fills_are_serialized() {
    let tmp = TempDir::new().unwrap();
    let cache = Arc::new(Cache::open_at(tmp.path().to_path_buf()).unwrap());
    let key = Arc::new(fresh_key("concurrent"));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let cache = Arc::clone(&cache);
        let key = Arc::clone(&key);
        let calls = Arc::clone(&calls);
        handles.push(thread::spawn(move || {
            cache
                .get_or_fill(&key, move |dir| {
                    // Sleep so racing threads pile on the lock.
                    thread::sleep(Duration::from_millis(50));
                    calls.fetch_add(1, Ordering::SeqCst);
                    std::fs::write(dir.join("concurrent.bin"), b"once")?;
                    Ok(())
                })
                .unwrap()
                .dir
        }));
    }
    let dirs: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "fill serialized to exactly once"
    );
    let first = &dirs[0];
    for d in &dirs[1..] {
        assert_eq!(d, first, "every thread sees the same cache dir");
    }
}

#[test]
fn key_with_listing_round_trips() {
    let tmp = TempDir::new().unwrap();
    let cache = Cache::open_at(tmp.path().to_path_buf()).unwrap();
    let mut key = fresh_key("with_lst");
    key.with_listing = true;
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_a = Arc::clone(&calls);
    cache
        .get_or_fill(&key, |dir| {
            calls_a.fetch_add(1, Ordering::SeqCst);
            std::fs::write(dir.join("with_lst.bin"), b"BIN")?;
            std::fs::write(dir.join("with_lst.lst"), b"LST")?;
            Ok(())
        })
        .unwrap();
    let calls_b = Arc::clone(&calls);
    let second = cache
        .get_or_fill(&key, |_| {
            calls_b.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(second.dir.join("with_lst.bin").is_file());
    assert!(second.dir.join("with_lst.lst").is_file());
    assert!(second.provenance.with_listing);
    assert!(second.provenance.listing_sha256.is_some());
}

#[test]
fn key_digest_is_stable_and_includes_args() {
    let a = CacheKey {
        tool_path: Utf8PathBuf::from("/x/y"),
        input_sha: "f00".into(),
        extra_args: vec!["--load-addr".into(), "0x010000".into()],
        output_stem: "s".into(),
        output_ext: "bin".into(),
        with_listing: false,
    };
    let b = CacheKey {
        extra_args: vec!["--load-addr".into(), "0x020000".into()],
        ..a.clone()
    };
    assert_ne!(
        a.digest(),
        b.digest(),
        "differing extra_args -> different digest"
    );
    let a2 = a.clone();
    assert_eq!(a.digest(), a2.digest(), "deterministic");
}
