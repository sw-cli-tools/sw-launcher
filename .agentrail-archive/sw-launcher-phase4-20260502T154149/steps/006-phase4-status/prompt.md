# Step 6: phase4-status

Standard phase close-out:

1. \`docs/status.md\`: mark Phase 4 done, add a \"Phase 4
   closure\" section enumerating all 6 step outcomes and the
   tool versions Phase 4 ran against. Refresh the
   carry-forward sw-checklist failure list to the current 14.
2. \`docs/saga-phase5-plan.md\` (new): seed Phase 5. Outline:
   - sub-crate-extract-validate (lift validate.rs into
     sw-launcher-validate; drops the 23-fn / 817-LOC fail).
   - sub-crate-extract-manifest (lift manifest.rs into
     sw-launcher-manifest; drops manifest.rs fn count fail).
   - heap-budget-enforcement (E0028..E0034 already wired in
     validate; phase 5 makes \`run\`/\`build\` consult them
     against the chosen memory_profile).
   - multi-target-stub-rca1802 (sw-launcher-target trait;
     stub implementation that just errors with E0050).
   - multi-target-stub-ibm1130 (same shape).
   - phase5-status (close-out).
3. README.md status section bumps to \"Phase 4 complete\"
   with pointer at the Phase 5 plan.
4. \`agentrail complete --done\`.

## Pre-commit gate

Standard. No code changes; existing test corpus must still
pass.

## Done when

- All gates green.
- docs/saga-phase5-plan.md is actionable (next saga can
  agentrail init --plan against it directly).
- README points readers at Phase 5.
- agentrail complete --done.

## After complete

Phase 5 opens with \`agentrail init --name sw-launcher-phase5
--plan docs/saga-phase5-plan.md\` in a fresh session.