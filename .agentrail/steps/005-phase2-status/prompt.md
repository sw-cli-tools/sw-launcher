# Step 5: phase2-status

Close the Phase 2 saga. Update `docs/status.md`, draft the Phase
3 saga seed (Scenario C: nested interpreter, OCaml/tuplet
shape), update README. No new Rust.

## What lands

- `docs/status.md`:
  - Phase table: Phase 2 -> done with date; cite cor24-run +
    pa24r versions Phase 2 integration tests ran against.
  - "Phase 2 closure" section enumerating each step's outcome.
- `docs/saga-phase3-plan.md` (new): Scenario C seed.
  - Cites `docs/survey/ocaml.md` (heap-limit-only patches,
    UART <source>+EOT+<stdin>) and `docs/survey/tuplet.md`
    (the failure-mode hypothesis -- tuplet's actual fix is
    sw-cor24-ocaml's GC work in flight; sw-launch should be
    able to express the layout once that lands).
  - 4-5 steps in the same shape as Phase 2:
    - heap-limit-patch-resolution (segment.patches with
      "self.end" expansion)
    - reserved-segments-loadplan (zero-fill bytes for
      heap/stack/bss segments not embedded in artifact)
    - uart-chunk-composition (multi-chunk UART with per-
      chunk terminators in declared order)
    - scenario-c-fixture-and-runner
    - phase3-status
- `README.md`: Status section updated to Phase 2 complete with
  pointer at Phase 3 plan.
- `agentrail complete --done`.

## Pre-commit gate

Standard. No code changes; cargo test must still pass on the
existing 60+ test corpus plus whatever Phase 2 added.

## Done when

- All gates green.
- `docs/saga-phase3-plan.md` is actionable enough that the
  next saga can `agentrail init --plan` directly against it.
- README points readers at Phase 3.
- `agentrail complete --summary "..." --reward 1 --done`.

## After complete

Phase 3 is opened with `agentrail init --name
sw-launcher-phase3 --plan docs/saga-phase3-plan.md` in a fresh
session. Do not start it here.
