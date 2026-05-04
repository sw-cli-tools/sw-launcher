# Step 2: sub-crate-extract-manifest

Same recipe as step 1, applied to manifest. Move src/manifest.rs
into a new crates/sw-launcher-manifest/ sub-crate so the main
crate's manifest.rs Module Function Count fail (8 fns) drops
off the per-crate budget. After step 1 the main crate has
re-exports of config and validate; step 2 adds manifest.

## What lands

- New crates/sw-launcher-manifest/ sub-crate.
  Cargo.toml deps: anyhow, camino, sw-launcher-config,
  sw-launcher-tool (for Listing).
- git mv src/manifest.rs -> crates/sw-launcher-manifest/src/lib.rs.
- git mv tests/manifest_unit.rs -> crates/sw-launcher-manifest/tests/.
- Main crate's lib.rs gains pub use sw_launcher_manifest as manifest.
- Manifest uses anyhow::Result internally. resolve_patch_term's
  Error::cli calls become anyhow! macros; the From<anyhow::Error>
  boundary in main crate's error.rs handles ?-conversion at
  every caller (cli.rs etc).
- crates/sw-launcher-manifest/tests/manifest_unit.rs imports
  re-pointed: sw_launcher::manifest -> sw_launcher_manifest.
  Fixture paths if any -> ../../tests/fixtures.

## Tests

All 117 tests still pass. No new tests in this step (pure
mechanical move).

## Pre-commit gate

Standard. Expected sw-checklist delta: -1 fail (manifest.rs
fn count 8 disappears from main crate). The same fail may
re-appear under the new sub-crate but counts as a separate
budget violation per the sw-checklist tooling.

## Done when

- sw-checklist drops the main-crate manifest.rs fn count fail.
- 117 tests still pass.
- Document the count delta in the step commit.

## Citations

- docs/saga-phase5-plan.md step 2.
- Phase 5 step 1 commit (dd8dd51) -- the validate extraction
  is the immediate reference.
- Phase 4 step 1 commit (8f04b46) -- the original tool
  extraction pattern.

## Scope guardrails

- Do NOT change manifest behaviour. This is pure code motion.
- Do NOT also extract cli.rs / doctor.rs / graph.rs --
  they are smaller and stay in the main crate for now.
- Do NOT introduce new error codes.