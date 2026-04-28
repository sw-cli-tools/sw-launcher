# sw-launcher Status

Last updated: 2026-04-28

## At a glance

- Repo created, on `main`, no published binary yet.
- Documentation drafted: PRD, architecture, design, plan, this file.
- Initial agentrail saga seeded with the Phase 1 step list (see
  `.agentrail/` for live state, `agentrail history` for prior step
  summaries).
- Code: `src/main.rs` is the cargo-new placeholder (`Hello, world!`).
- Pre-commit gates baseline: `sw-checklist` reports 6 passed.

## Phase progress

| Phase | Title                                  | State    |
|-------|----------------------------------------|----------|
| 0     | Survey existing repo layouts           | not started |
| 1     | Skeleton + Scenario A                  | not started |
| 2     | Scenario B (runtime + binary)          | not started |
| 3     | Scenario C (nested interpreter)        | not started |
| 4     | Caching, vendor sync, doctor, graph    | not started |
| 5     | Multi-target stubs (deferred)          | not started |

Phase 0 grounds the schema in observed reality across:
`sw-embed/sw-cor24-{apl,basic,forth,macrolisp,ocaml,pascal,plsw,
smalltalk,snobol4}` (each ships a working layout) and
`sw-vibe-coding/tuplet` (currently failing). Schema may be revised
after Phase 0 reveals gaps; the change shows up as an edit to
`docs/design.md` before Phase 1 step 2.

## Open questions parked for later

- Should symbol resolution for patches read `cor24-run --assemble`
  listings or a separate map-file format? (Decision deferred to Phase
  2 step 8.)
- Do we want a `[tools.*.version_match]` regex to allow tool version
  drift, or insist on exact `--version` strings? (Phase 4.)
- Do consuming repos commit `sw-launch.lock` or treat it like
  `Cargo.lock` for binaries (yes, commit)? Working assumption: commit.

## How to find current work

```
agentrail next      # what to do this session
agentrail status    # which step is in-progress
agentrail history   # what's been done
```

This file is updated as each phase closes. Per-step progress lives in
`.agentrail/` and in commit messages.
