# Step 9: phase1-status

Close out Phase 1 of the saga. Update status doc, draft Phase 2
saga seed grounded in the survey, mark this saga `--done`. No new
Rust except possibly trivial doc-comment updates.

## What lands

- `docs/status.md`: mark Phase 1 complete with the date; record the
  exact `cor24-run` and Rust toolchain versions that the Phase 1
  integration test ran against.
- `docs/saga-phase2-plan.md`: a draft step list for Phase 2
  (Scenario B) that:
  - cites the per-repo survey docs that informed each step
  - defines a fixture for Scenario B small enough to test against
    real `cor24-run` (a tiny `.spc` "hello" using the vendored
    pvm.s)
  - lists steps in the same shape as this saga (one paragraph per
    step, "What lands / Tests / Pre-commit / Done when / Stop")
- `README.md`: add a one-paragraph "Status" pointing readers at
  `docs/status.md`.

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

## Done when

- All gates green.
- `docs/saga-phase2-plan.md` is a complete, actionable plan that
  the next saga can use directly via `agentrail init`.
- Commit message marks Phase 1 closed.
- `agentrail complete --summary "..." --reward 1 --done`.

## After complete

Phase 2 is opened as a *new* saga with `agentrail init --name
sw-launcher-phase2 --plan docs/saga-phase2-plan.md`. Do not start
Phase 2 in this session.
