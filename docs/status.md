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

| Phase | Title                                  | State           |
|-------|----------------------------------------|-----------------|
| 0     | Survey existing repo layouts           | done 2026-04-28 |
| 0.5   | Schema v1.1 (fixed 8x128 KiB grid)     | done 2026-04-28 |
| 0.6   | Schema v1.2 (named memory profiles)    | done 2026-04-28 |
| 1     | Skeleton + Scenario A                  | scaffold landed; Scenario A not started |
| 2     | Scenario B (runtime + binary)          | not started     |
| 3     | Scenario C (nested interpreter)        | not started     |
| 4     | Caching, vendor sync, doctor, graph    | not started     |
| 5     | Multi-target stubs (deferred)          | not started     |

## Schema v1.1 (2026-04-28)

`docs/design.md` revised after the 13-repo survey. Every gap in
`docs/survey/schema-gaps.md` is addressed:

- **Addressing model**: 8 x 128 KiB partition grid with 4 x 32 KiB
  regions (`code`/`heap`/`spare`/`stack`) is the default; absolute
  addresses are an opt-out via `absolute_addresses = true`.
- **New layer kinds**: `composite`, `uart-preamble`,
  `uart-prebuffer`, `snapshot`, `regenerated`.
- **Run modes**: `batch | terminal | resident | echo-line`.
- **Programs slot table**: registry exposed by resident shells.
- **Shared regions**: declared per layer, validated globally.
- **Conditional loads**: file-presence / env predicates that
  toggle layers.
- **New scenario shapes**: D (composite image), E (resident
  shell + program slots).
- **Tool model**: `host-binary | emulator-hosted | script |
  composite`; `path | vendor | sibling | from_path` source kinds;
  named `post_process` transforms.
- **Patch value forms**: `self.*`, cross-layer segment refs,
  `sidecar:` references, cross-vendor symbol refs.
- **New validation codes**: E0017..E0027 covering partition
  collisions, mixed-mode collisions, sidecar staleness, shared-
  region overlap, regenerated drift, non-contiguous claims,
  resident-mode mismatch, cycle-budget outliers, composite-linker
  missing, per-file-on-non-uart, guard-too-small.
- **Out of scope (v1.1)**: filesystem stubs, GC scheduling,
  diff-snapshots, multi-target sweeps, CI metadata, signal
  handling.

## Schema v1.2 (2026-04-28)

`docs/design.md` revised again, replacing v1.1's fixed 8 x 128
KiB partition grid with named memory profiles. Rationale and
analysis: `docs/memory-stance.md` and `docs/heap-analysis.md`.

- **No fixed partition grid.** The `[targets.cor24.partitions]`
  block from v1.1 is removed.
- **`[memory_profiles.<name>]`** blocks declare per-use-case
  partition shapes and `budget` blocks (`code_max`, `heap_max`,
  `stack_max`, `total_max`).
- **Five default profile families**: `compiled-app`,
  `interpreter-only`, `repl-inline-compile`, `compiler-image`,
  `resident-shell`. Sized per `docs/heap-analysis.md`.
- **`heap_justification` block** required on any layer claiming
  heap > 32 KiB. Categories: `algorithmic-floor`,
  `bytecode-image`, `gc-slack` (accepted); `dead-leak`,
  `algorithmic-bloat` (warn; rejected by `--strict`).
- **Multi-heap, multi-stack profiles** supported -- a profile
  may declare any number of heap regions and stack regions of
  any size.
- **Layers cite profile partitions/regions by name**, not by
  absolute hex. `absolute_addresses = true` opt-out preserved
  for layers that don't fit (plsw monolith).
- **New validation codes E0028..E0034**: heap-budget overshoot,
  heap-budget 80% warn, missing justification, undeclared
  profile, region-not-in-profile, profile self-overlap, 1 MiB
  rule of thumb.
- **`--strict` mode** promotes warnings to errors except E0024
  and E0027, and *always* rejects `category = "dead-leak"` /
  `"algorithmic-bloat"`.

## Heap-analysis findings (informs every later phase)

Per `docs/heap-analysis.md`, the bloat is concentrated in three
repos:

- **ocaml** (~252 KiB heap, dead-leak): the
  `sw-cor24-ocaml#28` GC work in flight should bring this to <=
  64 KiB. Tuplet inherits the shrinkage.
- **macrolisp** (~288 KiB BSS, algorithmic-bloat): cell count
  oversized 16x; 1-byte mark bits when 1-bit would do. Target
  <= 64 KiB heap.
- **plsw** (~1 MiB compiler image, algorithmic-bloat):
  transpiler-output redundancy. UCSD Pascal compiled in 64 KiB;
  Turbo Pascal 1.0 in 33.5 KiB. Target <= 256 KiB.

Other repos (forth, basic, snobol4, smalltalk, sws, yocto-ed,
monitor) have algorithmic-floor or near-floor heap claims;
nothing to chase. Forth and basic have the smallest claims and
are closest to their historical implementations.

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
