# Step 6: phase3-status

Close the Phase 3 saga. Update docs/status.md, draft Phase 4
saga seed (disk cache + vendor lockfile + doctor + graph),
update README.md, mark `agentrail complete --done`.

## What lands

docs/status.md
  - Phase table marks Phase 3 done with the date.
  - "Phase 3 closure" section enumerates each step's outcome.
  - Records tool versions Phase 3 integration tests ran against.

docs/saga-phase4-plan.md (new)
  Phase 4 seed (disk cache + vendor + doctor + graph). Outline:

    1. disk-cache-layout       -- ~/.cache/sw-launch/ artifacts
                                  + per-layer keys + provenance.
                                  Replaces the in-process
                                  HashMap with a content-
                                  addressed disk cache that
                                  survives between runs.
    2. cache-explain-list-clean -- `sw-launch cache list /
                                  explain / clean` subcommands.
    3. vendor-lockfile         -- sw-launch.lock writer/reader;
                                  pin sibling-repo SHAs at
                                  `vendor sync` time;
                                  refuse stale lockfile at
                                  run time.
    4. doctor                  -- `sw-launch doctor` verifies
                                  cor24-run, pa24r, p24-load,
                                  vendored sources are
                                  reachable; reports versions.
    5. graph                   -- `sw-launch graph <scenario>`
                                  text + --json output of the
                                  layer DAG.
    6. phase4-status           -- close saga; seed Phase 5.

  Cite docs/design.md "Cache key", "Lockfile format",
  "CLI surface" sections.

README.md
  Status section bumps to "Phase 3 complete" with pointer at
  the Phase 4 plan.

## Pre-commit gate

Standard. No code changes; existing test corpus must still
pass.

## Done when

- All gates green.
- `docs/saga-phase4-plan.md` is actionable (next saga can
  `agentrail init --plan` against it directly).
- README points readers at Phase 4.
- `agentrail complete --summary "..." --reward 1 --done`.

## After complete

Phase 4 opens with `agentrail init --name sw-launcher-phase4
--plan docs/saga-phase4-plan.md` in a fresh session.
