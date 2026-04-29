# sw-launcher Status

Last updated: 2026-04-29

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
| 1     | Skeleton + Scenario A end-to-end       | done 2026-04-28 |
| 2     | Scenario B end-to-end (runtime + p-code) | done 2026-04-29 |
| 3     | Scenario C (nested interpreter)        | done 2026-04-29 |
| 4     | Caching, vendor sync, doctor, graph    | not started; plan in docs/saga-phase4-plan.md |
| 5     | Multi-target stubs (deferred)          | not started     |

## Phase 1 closure (2026-04-28)

Phase 1 saga (`sw-launcher-phase1`, 10 steps) closed cleanly. End
state:

- `sw-launch run echo --config tests/fixtures/scenario_a/sw-launch.toml`
  exits 0, prints "A" on stdout (the captured UART output).
- `sw-launch check echo` validates the scenario without spawning
  the emulator.
- `sw-launch build echo` assembles every assembler-kind layer
  under `<config-dir>/.sw-launch/build/<scenario>/<layer>/`.
- 60 tests across 11 binaries, including 2 end-to-end against the
  real `cor24-run` binary, all green.

### Versions the Phase 1 integration tests ran against

| component                | version                              |
|--------------------------|--------------------------------------|
| `cor24-run`              | 0.1.0 (Copyright (c) 2026 Michael A Wright; MIT) |
| Rust toolchain           | rustc 1.94.1 (e408947bf 2026-03-25)  |
| edition                  | 2024                                 |
| host                     | darwin (Darwin 24.6.0, manager)      |

### Steps closed in this saga

1. `001-survey-repos` -- 13 per-repo surveys + index + schema gaps
   + tuplet failure hypothesis + monitor-shell feasibility +
   partition-model proposal + web/memory-layouts/index.html.
2. `002-revise-schema` -- design.md v1.1: addressing model,
   composite/uart-preamble/uart-prebuffer/snapshot/regenerated
   layer kinds, run modes, programs slot table, shared regions,
   conditional loads, tool model, sidecar/upstream-symbol patches,
   Scenario D + E examples, validation codes E0017..E0027.
3. `003-scaffold-cli` -- clap-driven binary, error + ErrorCode,
   `--version` with build-info, `--help` with AI agent section,
   14 integration tests + unit tests.
4. `004-schema-v1.2-variable-partitions` (inserted) --
   docs/heap-analysis.md grounding budgets in historical
   benchmarks; design.md replaces fixed grid with named memory
   profiles + `heap_justification` + budgets E0028..E0034.
5. `005-config-types` -- typed Config tree mirroring v1.2 with
   strict deserialization, `HexValue` / `SizeOrAuto`, fixtures
   for Scenarios A and B.
6. `006-scenario-validate` -- 17 stable error codes implemented
   with one negative test each; `cli::check` wired up.
7. `007-assembler-tool` -- cor24-run `--assemble` wrapper with
   in-process memoization (sha256 cache key) + listing parser
   handling trailing labels (BSS / .skip).
8. `008-scenario-a-loadplan` -- `LoadPlan::build` + `cor24_argv`
   with deterministic ordering; embedded segments tracked but
   not loaded; 5 snapshot/ordering tests.
9. `009-scenario-a-runner` -- `cli::run` orchestrates
   validate -> assemble -> plan -> spawn -> capture UART ->
   check expectations; `regex` dep for `uart_regex`; 2 end-to-end
   tests against real `cor24-run`.
10. `010-phase1-status` -- this entry; closes the saga and
    drafts `docs/saga-phase2-plan.md`.

### What runs today

```bash
cd tests/fixtures/scenario_a
sw-launch run echo
# -> "A"
# exit 0
```

### Outstanding sw-checklist failures (carried into Phase 2)

These are documented structural trade-offs, not regressions; see
the step 006 / 008 commit messages for rationale:

- `validate.rs` File LOC: 642 (max 500). Splitting forces
  Crate Module Count over its own cap; the 17-rule validation
  surface doesn't decompose cleanly into smaller files.
- `validate.rs` Module Function Count: 20 (max 7). Same root.
- Crate Module Count: 8 (max 7). `manifest.rs` is a genuinely
  separate concern between Config/validate and the runtime
  emitter. Folding it into any existing module pushes that
  module over its own per-file budgets.

Phase 2 may break the validation logic into a sub-crate (e.g.
`sw-launcher-validate`) once the rule count grows further; until
then, the documentation in step commits is the contract.

## Phase 2 closure (2026-04-29)

Phase 2 saga (`sw-launcher-phase2`, 5 steps) closed cleanly.
Scenario B runs end to end: pvm.bin@0 + hello.p24m@0x010000 +
`code_ptr` patch resolved through pvm.lst, producing UART
output `"PVM OK\nHello\nHALT"` from the real emulator.

### Versions the Phase 2 integration tests ran against

| component                | version                              |
|--------------------------|--------------------------------------|
| `cor24-run`              | 0.1.0 (Copyright (c) 2026 Michael A Wright; MIT) |
| `pa24r`                  | sw-cor24-pcode/target/release (sibling) |
| `p24-load`               | sw-cor24-pcode/target/release (sibling) |
| `pvm.s`                  | sw-cor24-pcode/vm/pvm.s (sibling)    |
| Rust toolchain           | rustc 1.94.1 (e408947bf 2026-03-25)  |

### Steps closed in this saga

1. `001-pcode-tool` -- unified `Tool` struct with
   `ToolKind::Assembler|Pcode`; `SourceSpec::Path|FromPath|Sibling`
   resolution; pa24r integration test; backward-compat
   `Assembler` type alias retained.
2. `002-listing-symbol-resolution` -- E0006 promoted to fully
   resolved; cross-layer `<layer>.<symbol>` patches resolve
   through `Listing.resolve` + load address; Wagner-Fischer
   Levenshtein "did you mean" suggestions for typos in layer
   and symbol names.
3. `003-pcode-image-layer` -- `assemble_artifacts` dispatches by
   `Layer.kind`: assembler -> Tool(Assembler); pcode -> Tool
   (Pcode) lazy-init; binary / pcode-image -> pass-through.
   `build_scenario` rewritten to call shared `assemble_artifacts`.
4. `004-scenario-b-fixture-and-runner` -- `tests/scenario_b.rs`
   pre-builds pvm.bin + hello.p24m via cor24-run + pa24r +
   p24-load, generates a tempdir TOML with kind=pcode-image
   pass-through, runs `sw-launch run pcode-hello` end to end.
   Two tests (happy + misaligned-patch); both pass on the dev
   machine when all four tools are present.
5. `005-phase2-status` -- this entry; closes the saga and
   drafts `docs/saga-phase3-plan.md`.

### What runs today (Phase 2)

```bash
# Test fixture pre-builds pvm.bin / hello.p24m, then:
sw-launch run pcode-hello --config <tempdir>/sw-launch.toml
# -> "PVM OK\nHello\nHALT"
# exit 0
```

### Carry-forward sw-checklist failures (7)

Same root constraint conflict documented in step 006 / 008 /
Phase-2-step-1. The validation + manifest + cli-orchestration
surface area doesn't decompose cleanly into the per-file caps
without pushing the crate-module count over its own cap.
Possible Phase 5 remediation: extract `validate` + `tool` + 
`manifest` into sub-crates so each gets its own per-module
budget.

  File LOC [validate.rs]:                           744 lines
  Module Function Count [validate.rs]:              23 fns
  Module Function Count [manifest.rs]:              8 fns
  Function LOC ['build' in tool.rs]:                53 lines
  Function LOC ['check_patch_term' in validate.rs]: 61 lines
  Function LOC ['assemble_artifacts' in cli.rs]:    88 lines
  Crate Module Count:                               8 modules

## Phase 3 closure (2026-04-29)

Phase 3 saga (`sw-launcher-phase3`, 6 steps) closed cleanly.
Scenario C runs end to end: pvm.bin@0 + ocaml.p24m@0x040000
with `code_ptr` and `heap_limit` patches read from the
sw-cor24-ocaml repo's sidecar files; OCaml source delivered
through UART with EOT terminator. The real emulator evaluates
`let x = 1 + 2 in print_int x` and the captured UART payload
is `"PVM OK\n> let x = 1 + 2 in print_int x\n3\nHALT"`.

### Versions the Phase 3 integration tests ran against

| component                | version                              |
|--------------------------|--------------------------------------|
| `cor24-run`              | 0.1.0 (Copyright (c) 2026 Michael A Wright; MIT) |
| `pa24r`                  | sw-cor24-pcode/target/release (sibling) |
| `p24-load`               | sw-cor24-pcode/target/release (sibling) |
| `pvm.bin`                | sw-cor24-ocaml/build/pvm.bin (vendored from sibling build) |
| `ocaml.p24m`             | sw-cor24-ocaml/build/ocaml.p24m (Pascal -> p-code, p24-load-linked) |
| Rust toolchain           | rustc 1.94.1 (e408947bf 2026-03-25)  |

### Steps closed in this saga

1. `001-pcode-linker` -- `ToolKind::PcodeLinker` added;
   `assemble_artifacts` extends the pcode arm to chain
   pa24r .spc -> .p24, then p24-load --load-addr <addr> ->
   .p24m, dropping the test-fixture pre-link.
2. `002-sidecar-patch-values` -- `value = "sidecar:<path>"`
   patch form (schema gap B1); `LoadPlan::build` reads the
   file relative to config_dir, parses one hex literal
   (with or without `0x`); negative tests for missing file
   and non-hex contents.
3. `003-reserved-heap-segments` -- non-embedded
   `kind = "heap" | "stack" | "bss"` segments contribute to
   `LoadPlan.segments` (and overlap accounting / E0003)
   without entering `memory_loads`; cor24-run's default
   zero-init handles the range without an extra argv flag.
4. `004-uart-chunk-composition` -- multi-chunk UART payloads
   concatenate in declared order, each chunk followed by
   its terminator (EOT / ETX / EOF / none); golden-hex tests
   for the byte-string the emulator receives.
5. `005-scenario-c-fixture-and-runner` --
   `tests/fixtures/scenario_c/demo.ml` with
   `let x = 1 + 2 in print_int x` + `tests/scenario_c.rs`:
   end-to-end test against real cor24-run + ocaml build
   artifacts; oversized-source test fires E0004 via
   `sw-launch check`. Validate's `check_patch_term`
   recognizes the `sidecar:` prefix (was previously misread
   as an undeclared cross-layer symbol).
6. `006-phase3-status` -- this entry; closes the saga and
   drafts `docs/saga-phase4-plan.md`.

### What runs today (Phase 3)

```bash
# In a tempdir set up by tests/scenario_c.rs:
sw-launch run nested-demo --config <tempdir>/sw-launch.toml
# -> "PVM OK"
# -> "> let x = 1 + 2 in print_int x"
# -> "3"
# -> "HALT"
# exit 0
```

### Carry-forward sw-checklist failures (9)

Same root constraint conflict documented in step 006 / 008 /
Phase 2 step 1. The validation + manifest + cli-orchestration
surface area doesn't decompose cleanly into the per-file caps
without pushing the crate-module count over its own cap.
Phase 4 step 1 begins the documented remediation by extracting
`tool` + `cache` into a `sw-launcher-tool` sub-crate; Phase 5
finishes the split for `validate` and `manifest`.

  File LOC [validate.rs]:                              817 lines
  File LOC [cli.rs]:                                   526 lines
  Module Function Count [validate.rs]:                 23 fns
  Module Function Count [manifest.rs]:                 8 fns
  Function LOC ['build' in tool.rs]:                   53 lines
  Function LOC ['resolve_patch_term' in manifest.rs]:  62 lines
  Function LOC ['check_patch_term' in validate.rs]:    62 lines
  Function LOC ['assemble_artifacts' in cli.rs]:       120 lines
  Crate Module Count:                                  8 modules

### Test-corpus growth across phases

| phase  | tests | binaries | new end-to-end fixtures   |
|--------|-------|----------|---------------------------|
| Phase 1 |  60  |  11      | scenario_a (echo, UART)   |
| Phase 2 |  73  |  15      | scenario_b (pvm + p-code) |
| Phase 3 |  86  |  16      | scenario_c (nested OCaml) |

## What's next

Phase 4 (`sw-launcher-phase4`) is described in
[`docs/saga-phase4-plan.md`](saga-phase4-plan.md). It hardens
what Phases 1-3 produced rather than adding new scenario
shapes:

1. Disk-persistent content-addressed cache under
   `~/.cache/sw-launch/` (replaces the in-process HashMap).
2. `cache list / explain / clean` subcommands.
3. `sw-launch.lock` writer/reader + drift detection
   (E0040..E0042).
4. `sw-launch doctor` host-environment checker.
5. `sw-launch graph <scenario>` text + `--json` DAG output.
6. Status close-out + Phase 5 seed.

Phase 4 step 1 begins the sub-crate extraction (`sw-launcher-
tool`) that the running sw-checklist trade-off list
documents as the planned remediation.

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
