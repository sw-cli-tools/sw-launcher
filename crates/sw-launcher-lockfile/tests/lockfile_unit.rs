//! Unit tests for `sw_launcher_lockfile`.
//!
//! Phase 4 step 3: cover the three behaviours from the step
//! plan -- deterministic write, drifted-input detection, and
//! `update_only_drifted` keeping fresh entries' synced_at.

use camino::Utf8PathBuf;
use sw_launcher_lockfile::{EntryStatus, Lockfile, VendorInput, sync, update_only_drifted};
use tempfile::TempDir;

fn write_input(dir: &TempDir, name: &str, contents: &[u8]) -> Utf8PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    Utf8PathBuf::from_path_buf(path).unwrap()
}

fn input(key: &str, path: Utf8PathBuf) -> VendorInput {
    VendorInput {
        key: key.to_string(),
        resolved_path: path,
        vendor_repo: None,
    }
}

#[test]
fn vendor_sync_writes_deterministic_lockfile() {
    let tmp = TempDir::new().unwrap();
    let a = write_input(&tmp, "a.bin", b"AAA");
    let b = write_input(&tmp, "b.bin", b"BBB");
    let inputs = vec![input("a", a), input("b", b)];
    let now = "2026-04-30T12:00:00Z";
    let l1 = sync(&inputs, now).unwrap();
    let l2 = sync(&inputs, now).unwrap();
    assert_eq!(l1.to_toml_string(), l2.to_toml_string());
    assert_eq!(l1.vendored.len(), 2);
    assert_eq!(l1.vendored[0].key, "a");
    assert_eq!(l1.vendored[1].key, "b");
    // Round-trip through TOML.
    let serialized = l1.to_toml_string();
    let parsed: Lockfile = toml::from_str(&serialized).unwrap();
    assert_eq!(parsed, l1);
}

#[test]
fn drifted_input_is_reported_as_drifted() {
    let tmp = TempDir::new().unwrap();
    let p = write_input(&tmp, "drift.bin", b"original");
    let inputs = vec![input("drift", p.clone())];
    let lock = sync(&inputs, "2026-04-30T12:00:00Z").unwrap();
    // Mutate the file under the lockfile's feet.
    std::fs::write(p.as_std_path(), b"tampered").unwrap();
    let statuses = lock.status();
    assert_eq!(statuses.len(), 1);
    match &statuses[0].1 {
        EntryStatus::Drifted { recorded, observed } => {
            assert_ne!(recorded, observed);
            assert_eq!(recorded.len(), 64);
            assert_eq!(observed.len(), 64);
        }
        other => panic!("expected Drifted, got {other:?}"),
    }
}

#[test]
fn update_only_drifted_preserves_fresh_synced_at() {
    let tmp = TempDir::new().unwrap();
    let stable = write_input(&tmp, "stable.bin", b"unchanged");
    let mutable = write_input(&tmp, "mutable.bin", b"v1");
    let inputs = vec![
        input("stable", stable.clone()),
        input("mutable", mutable.clone()),
    ];
    let prior = sync(&inputs, "2026-04-30T12:00:00Z").unwrap();
    // Drift only the mutable entry.
    std::fs::write(mutable.as_std_path(), b"v2").unwrap();
    let updated = update_only_drifted(&prior, &inputs, "2026-05-01T12:00:00Z").unwrap();

    let stable_entry = updated.vendored.iter().find(|e| e.key == "stable").unwrap();
    let mutable_entry = updated
        .vendored
        .iter()
        .find(|e| e.key == "mutable")
        .unwrap();
    assert_eq!(
        stable_entry.synced_at, "2026-04-30T12:00:00Z",
        "fresh entry should keep its original synced_at"
    );
    assert_eq!(
        mutable_entry.synced_at, "2026-05-01T12:00:00Z",
        "drifted entry gets the new synced_at"
    );
    let prior_mutable_sha = prior
        .vendored
        .iter()
        .find(|e| e.key == "mutable")
        .unwrap()
        .input_sha256
        .clone();
    assert_ne!(mutable_entry.input_sha256, prior_mutable_sha);
}

#[test]
fn unresolvable_path_is_reported() {
    let tmp = TempDir::new().unwrap();
    let p = write_input(&tmp, "gone.bin", b"x");
    let inputs = vec![input("gone", p.clone())];
    let lock = sync(&inputs, "2026-04-30T12:00:00Z").unwrap();
    std::fs::remove_file(p.as_std_path()).unwrap();
    let statuses = lock.status();
    assert!(matches!(statuses[0].1, EntryStatus::Unresolvable));
}

#[test]
fn lockfile_round_trips_through_disk() {
    let tmp = TempDir::new().unwrap();
    let p = write_input(&tmp, "io.bin", b"contents");
    let inputs = vec![input("io", p)];
    let lock = sync(&inputs, "2026-04-30T12:00:00Z").unwrap();
    let path = tmp.path().join("sw-launch.lock");
    lock.write(&path).unwrap();
    let loaded = Lockfile::read(&path).unwrap();
    assert_eq!(loaded, lock);
    assert_eq!(Lockfile::read_optional(&path).unwrap(), Some(lock));
    let absent = tmp.path().join("nope.lock");
    assert_eq!(Lockfile::read_optional(&absent).unwrap(), None);
}
