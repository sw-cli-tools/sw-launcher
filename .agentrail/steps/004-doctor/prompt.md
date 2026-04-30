# Step 4: doctor

`sw-launch doctor` runs every gating check the Phase 1-3
integration tests do by hand and reports the results in a
single table. Lets users (and agents) ask one question --
\"can sw-launch run my fixtures right now?\" -- before they
hit cor24-run not found, ocaml.p24m missing, or some other
PATH issue.

## What lands

### Subcommand surface

```
sw-launch doctor [--config <path>] [--json]
```

- Without --config: runs only the host-tool checks.
- With --config: also walks the manifest's layers and
  verifies every layer.input + sidecar path resolves on disk;
  reads the lockfile if present and reports any drift.

### Checks (each prints a labeled row, PASS / WARN / FAIL)

1. \`cor24-run --version\` succeeds. Records the version
   string. FAIL if not on PATH or exits non-zero.
2. \`pa24r\` resolvable on PATH. (Calling --version fails on
   pa24r so just check is_file). WARN if missing (only
   needed for pcode layers).
3. \`p24-load\` resolvable on PATH. WARN if missing.
4. \$SW_LAUNCH_CACHE_DIR / \$XDG_CACHE_HOME /
   \$HOME/.cache/sw-launch is writable. Tries to create
   the dir + write a probe file. FAIL on permission denied.
5. (config supplied) every layer.input file resolves on
   disk. FAIL with the missing path's E0042 (or its own
   doctor-local code).
6. (config supplied) every sidecar:<path> resolves on disk.
7. (config supplied) sw-launch.lock present and every entry
   is fresh (re-uses Lockfile::status from step 3).

### Output

Default: human-readable table. \`--json\` emits a structured
report so CI can act on it.

Exit code: 0 iff every required check is PASS or WARN. Any
FAIL is exit 1.

### Module placement

- Option A: src/doctor.rs in the main crate. Adds 1 to the
  main-crate fn count; the doctor surface is small (~5
  helpers, ~150 LOC) so this is the lighter touch.
- Option B: new sw-launcher-doctor sub-crate.

Pick A this step (the doctor is naturally a thin orchestrator
over Cache, Lockfile, and Tool::find_binary_on_path). Keeping
it in-crate avoids another workspace member for what is
mostly a presentation layer. Document the choice; revisit if
doctor grows into something Phase 5 would benefit from
extracting.

## Tests (3 new + smoke)

- tests/doctor_cli.rs:
  1. doctor_with_no_config_passes_when_cor24_present
     (gated on cor24-run on PATH; otherwise skipped with a
     stderr note like Phase 1-3 fixtures).
  2. doctor_with_drifted_lockfile_fails_with_e0041
     (uses the vendor_cli pattern: write manifest, sync,
     tamper, run doctor, expect failure).
  3. doctor_json_emits_structured_array.
- tests/cli.rs's existing \`doctor_returns_not_implemented\`
  is replaced with a smoke test that exits 0 if cor24-run
  is on PATH, exit 1 otherwise.

## Pre-commit gate

Standard. The 12 carry-forward sw-checklist failures should
not regress; doctor's helpers in cli.rs (since cli.rs is
already over fn-count cap) or in src/doctor.rs (preferred --
lifts cli.rs and lifts the per-fn-count overage by one
module of overhead).

## Done when

- \`sw-launch doctor\` exits 0 on a clean dev machine with
  cor24-run + pa24r + p24-load on PATH.
- doctor exit 1 with E0041 on a drifted lockfile.
- doctor --json round-trips through a known-good schema.

## Citations

- docs/saga-phase4-plan.md step 4.
- docs/design.md \"CLI surface\" (line 1477).

## Scope guardrails

- Do NOT add network-aware checks (no \"is the upstream
  repo reachable?\"); doctor is host-local only.
- Do NOT replicate vendor status; doctor calls into the same
  Lockfile::status when --config is supplied.
- The doctor should not itself spawn cor24-run on a fixture;
  --version checks only.