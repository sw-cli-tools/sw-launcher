# sw-launcher Phase 5: sub-crate split + multi-target stubs

This is the saga seed for `sw-launcher-phase5`. Open with:

```bash
agentrail init --name sw-launcher-phase5 --plan docs/saga-phase5-plan.md
```

## Goal

Phase 4 hardened the operational surface (cache, lockfile,
doctor, graph) and brought the working test corpus to 117
tests / 25 binaries across three sub-crates
(`sw-launcher-tool`, `sw-launcher-lockfile`) and the binary.
Phase 4 also accumulated 14 sw-checklist failures, all
documented as the same root constraint: the main crate's
`validate.rs` (817 LOC, 23 fns) and `manifest.rs` (8 fns) are
too large for the per-module caps, and Phase 4 added
`doctor.rs` and `graph.rs` to the main crate, pushing the
crate-module count to 8 (cap 7).

Phase 5 does the cleanup the prior phases deferred:

1. **Extract `validate` and `manifest` into sub-crates.**
   This is the documented Phase-1-3 remediation. After
   extraction the main crate drops to 4-5 modules and
   `validate` / `manifest` each get their own per-crate fn /
   LOC budget. Most of the carry-forward sw-checklist
   failures resolve.
2. **Wire heap-budget enforcement into `run`/`build`.**
   E0028..E0034 are validation rules that already fire in
   `sw-launch check`. Phase 5 step 3 makes them block runs
   too, so a scenario that overruns its declared
   `memory_profile` budget can't silently ship.
3. **Stub two additional emulator backends.** The schema's
   `kind = "emulator" | ...` axis assumed multi-target from
   Phase 0; Phase 5 introduces the `Target` trait + two
   `not-yet-implemented` backends (RCA 1802, IBM 1130). The
   stubs error with a stable code (E0050) and exist so
   `sw-launch.toml` can name them as a target without a
   schema change later.

After Phase 5, Phase 6 (open-ended): pick which scenario
shape gets a real second backend.

## Working agreement

Same as Phases 1-4:

- TDD red/green/refactor for every step.
- Standard pre-commit gate green at every step's commit.
  Phase 5's *purpose* is to drop the running 14
  sw-checklist failures; each step should report the count
  delta in its commit message.
- Commit before `agentrail complete`. Push after `complete`
  (then again after the finalize commit).
- Module count target by end of Phase 5: 5 main-crate
  modules (`cli`, `config`, `doctor`, `graph`, `error`),
  4 sub-crates (`sw-launcher-tool`, `sw-launcher-lockfile`,
  `sw-launcher-validate`, `sw-launcher-manifest`,
  `sw-launcher-target`).

## Required ground truth (probe before step 1)

Before starting step 1, confirm the validate sub-crate
extraction is straightforward by listing every `pub` symbol
the main crate consumes from `validate`:

```bash
grep -nE "validate::|crate::validate" src/*.rs
```

The expected surface: `validate(cfg, scenario)` (the entry
point) plus `Severity`, `Diagnostic`, `ErrorCode`. Anything
else is a sign the main crate has a hidden coupling that
needs to lift before extraction.

## Step list

### Step 1 -- sub-crate-extract-validate

**What lands**

- New `crates/sw-launcher-validate/` sub-crate. Mirrors the
  Phase 4 step 1 / step 3 pattern (anyhow internally,
  re-exported through the main crate's `lib.rs` so prior
  `sw_launcher::validate::*` paths keep working).
- `validate.rs` moved (`git mv`) to preserve history.
- `Diagnostic`, `Severity`, `ErrorCode` move with it.
  `validate(cfg, scenario)` is the single public entry.
- The main crate's `From<anyhow::Error> for Error` already
  exists from Phase 4 step 1; no new boundary work.

**Tests**

- Existing `tests/validate_unit.rs` re-points to
  `sw_launcher::validate::*` (re-export path) and continues
  to pass with no behaviour change.
- Add 1 doc-comment test verifying the sub-crate exposes
  `validate` at the top level.

**Done when**

- 117 tests still pass after extraction.
- sw-checklist drops 4 fails: validate.rs File LOC,
  validate.rs Module Function Count, check_patch_term LOC
  (now under sub-crate's per-fn budget? probably still over,
  but reports against the sub-crate not main crate), and
  Crate Module Count drops back to 7 (or lower).
- Document the count delta in the step commit.

### Step 2 -- sub-crate-extract-manifest

**What lands**

- New `crates/sw-launcher-manifest/` sub-crate. Same shape
  as step 1.
- `manifest.rs` moved with history preserved. `LoadPlan`,
  `Artifacts`, `ArtifactEntry`, and the patch-term resolver
  all move.
- The main crate consumes via `pub use`.

**Tests**

- `tests/manifest_unit.rs` continues to pass against the
  re-exported path.

**Done when**

- 117 tests still pass.
- sw-checklist drops 2-3 more fails (manifest.rs fn count;
  resolve_patch_term LOC may shift but still fail; the
  drop is from "manifest.rs in main crate over fn cap"
  becoming "sw-launcher-manifest under per-crate budget").

### Step 3 -- heap-budget-enforcement

**What lands**

- `run_scenario` and `build_scenario` invoke
  `validate::validate` with strict mode set so heap-budget
  warnings (E0030 80% threshold) and errors
  (E0028 budget overrun, E0029 per-region overrun) block
  the run.
- `--no-strict` flag added to `run`/`build` so users who
  *want* to ship over-budget can opt out (with a stderr
  warning).
- New `tests/heap_budget.rs`:
  1. heap-overrun fixture is rejected by `run`.
  2. same fixture with `--no-strict` runs but warns on
     stderr.
  3. heap-warn-threshold (>80%) prints a warning on stderr
     even without --no-strict.

**Done when**

- A scenario whose layer claims a heap region larger than
  its profile's budget exits non-zero before any tool
  spawns.
- `--no-strict` documented in the help text.

### Step 4 -- multi-target-stub-rca1802

**What lands**

- New `crates/sw-launcher-target/` sub-crate. `Target` trait
  with `name() -> &str`, `argv(&LoadPlan) -> Vec<String>`,
  `run(&self, plan, scen) -> Result<(String, i32)>`.
- COR24 backend extracted from cli.rs's `run_emulator` into
  this trait's first impl.
- Stub `Target::Rca1802` impl that returns
  `Err(Error::cli("E0050 RCA 1802 backend not implemented"))`
  on every call.
- Schema gains a `target.kind = "rca1802"` value the
  validator now accepts (currently `"emulator"` is the only
  allowed kind for COR24-shaped targets).
- Two new error codes:
  - E0050 target-not-implemented (stub backend)
  - E0051 target-kind-mismatch (validator caught a
    target with kind "rca1802" but the binary doesn't
    have it compiled in)

**Tests**

- Manifest with `target = "rca1802"` is rejected at
  `run`/`build` time with E0050.
- Manifest with `target = "rca1802"` passes `check`
  (validation only -- the config is well-formed; the
  backend just isn't there yet).

**Done when**

- A user can declare an RCA 1802 target in
  `sw-launch.toml` without a schema error.
- Running it errors with E0050.

### Step 5 -- multi-target-stub-ibm1130

Same shape as step 4 but for IBM 1130. Reuses the `Target`
trait. Smaller diff because step 4 introduces the trait;
step 5 just adds a second impl.

**Done when**

- A user can declare `target = "ibm1130"` in
  `sw-launch.toml`.
- Running it errors with E0050.

### Step 6 -- phase5-status

Standard close-out: `docs/status.md` updates, draft Phase 6
saga seed (open-ended -- pick one of the stub backends to
implement, or pick a different milestone), `README.md`,
`agentrail complete --done`.

## Survey citations

- `docs/design.md` "Tool model" (lines 1375-1413) -- target
  kinds and resolution.
- `docs/architecture.md` -- the multi-target design that
  Phase 0 sketched.

## Risks and mitigations

- **Sub-crate extraction surfaces hidden couplings.** Phase
  4 step 1 already proved the pattern works
  (`sw-launcher-tool`); steps 1-2 follow the same recipe.
  If a step finds the coupling is too deep to lift in one
  step, fall back to the in-crate path and document.
- **Heap-budget enforcement breaks Phase 1-3 fixtures.**
  The Phase 1-3 fixtures were built before the budget rules
  fired in `run`. If they overrun, fix the fixture's
  declared profile or `heap_justification`. The fix is
  the right answer; the rules exist for a reason.
- **Multi-target stubs accidentally over-promise.** The E0050
  message must be specific: "RCA 1802 backend not
  implemented (sw-launcher-target/rca1802 is a stub; see
  Phase 6)". Don't print "coming soon" or pretend it's
  almost ready.

## Function/module-count budget

Phase 5's *purpose* is to bring the running sw-checklist
failure count down. The expected delta:

- Step 1 (extract validate): -3 fails (validate.rs LOC,
  validate.rs fn count, Crate Module Count if doctor /
  graph in main crate).
- Step 2 (extract manifest): -1 fail (manifest.rs fn count).
- Step 3 (heap budget): +0 fails (presentation logic only).
- Step 4 (rca1802 stub + Target trait): -1 fail (extracts
  cli.rs's run_emulator into a sub-crate; cli.rs fn count
  drops).
- Step 5 (ibm1130 stub): +0 fails (small diff).
- Step 6 (status): +0.

End-of-phase target: 14 -> 9 fails. The remainder are
Function LOC fails (62-line `check_patch_term`,
`resolve_patch_term`, 126-line `assemble_artifacts`,
55-line `build_via_disk`) that don't decompose cleanly and
remain documented trade-offs.
