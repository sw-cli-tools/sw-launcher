# Step 3: vendor-lockfile

Add sw-launch.lock so sibling-repo dependencies are pinned
between machines and CI. Refuse to run with a stale lockfile;
`vendor sync` resolves and writes the lockfile, `vendor
status` reports drift.

## What lands

### sw-launch.lock format

A TOML file next to sw-launch.toml. One [[vendored]] table per
distinct vendored input across all layers in every scenario:

```toml
schema_version = 1

[[vendored]]
key           = "sw-cor24-pcode/vm/pvm.s"        # repo-rel
resolved_path = "/abs/path/at/sync/time/pvm.s"   # absolute
vendor_repo   = "sw-embed/sw-cor24-pcode"        # if known
vendor_sha    = "abc1234..."                     # if .git
input_sha256  = "..."                            # 64 hex
synced_at     = "2026-04-30T12:00:00Z"
```

`key` is a stable identifier the lockfile is sorted by.
For SourceSpec::FromPath bins, key = binary name. For
Sibling layers, key = path-rel-to-config-dir.

### CLI surface

- `sw-launch vendor sync [--config <path>]`
  Walks every layer in every declared scenario, resolves
  each `input` / tool source (Path / FromPath / Sibling),
  computes input_sha256 (uses Tool::hash_file_sha256),
  reads vendor_sha from `.git/HEAD` if present, writes
  sw-launch.lock with sorted [[vendored]] entries and a
  fixed RFC-3339 synced_at.
- `sw-launch vendor status [--config <path>]`
  Loads the lockfile + walks the same inputs. Prints one
  line per vendored entry: fresh / drifted / missing /
  unverified. Exit 0 iff every entry is `fresh`.
- `sw-launch run` and `sw-launch build` consult the
  lockfile if present; refuse to proceed on drift unless
  `--update-lock` is passed.

### Validation codes

- E0040 lockfile-missing (run / build with no lockfile)
- E0041 lockfile-stale (input sha drifted)
- E0042 lockfile-unresolvable (vendor path doesn't exist)

### Module placement

Where to put the lockfile reader/writer:
- Option A: new src/lockfile.rs in the main crate. Adds 1 to
  the main-crate fn count which is already at 14; ~120 LOC
  on top of cli.rs at 726 -> still over but not worse than
  step 2.
- Option B (preferred): new sub-crate
  `crates/sw-launcher-lockfile`. Mirrors the Phase 4 step 1
  decision -- each new top-level concern goes in its own
  sub-crate so it gets its own fn-count budget.

Pick B unless workspace-conversion overhead is too high; in
that case fall back to A and document the deferral in the
step commit.

### Tests (3 new manifest-shaped + 3 CLI integration)

Manifest-level (new tests/lockfile_unit.rs OR sub-crate
tests):
1. vendor_sync_writes_deterministic_lockfile (sorted keys,
   fixed-clock synced_at).
2. drifted_input_fires_e0041 (sw-launch run after editing
   a vendored file errors with E0041).
3. update_lock_rewrites_only_drifted_entries (fresh entries
   keep their synced_at).

CLI-level (tests/vendor_cli.rs):
4. vendor_sync_on_phase1_fixture_succeeds.
5. vendor_status_after_sync_reports_all_fresh.
6. vendor_status_after_tampering_reports_drifted (exit 1).

## Pre-commit gate

Standard. The 11 carry-forward sw-checklist failures should
not regress. New module(s) preferentially go in their own
sub-crate.

## Done when

- sw-launch.lock format roundtrips through writer + reader.
- vendor sync produces byte-identical lockfile across runs
  given the same inputs (modulo synced_at).
- vendor status detects drift, missing, unverified states.
- sw-launch run refuses E0041 on drifted lockfile; gates
  pass on the existing fixtures after a one-time sync.

## Citations

- docs/design.md "Lockfile format" (lines 1196-1226).
- docs/saga-phase4-plan.md step 3.

## Scope guardrails

- Do NOT add automatic lock regeneration in `run` (that's
  --update-lock; default behaviour is to refuse).
- Do NOT cover non-vendored layers (text inputs colocated
  with sw-launch.toml are not "vendored" in the lockfile
  sense; they're project-local).
- Sidecar files (Phase 3 step 2) are vendored too --
  include their input_sha256.