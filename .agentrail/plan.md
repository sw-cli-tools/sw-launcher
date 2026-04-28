# sw-launcher: build the host-side reproducible scenario launcher

## Goal

Implement `sw-launch`, a Rust CLI that turns a layered emulator
scenario (described in `sw-launch.toml`) into one boring,
reproducible command. First target: COR24, driven through
`cor24-run`. The tool must support three primitive scenario shapes:

- A: assembled program at address 0, data via UART
- B: COR24 runtime at 0 + p-code/.p24 blob at higher address, with
  `code_ptr` patch
- C: runtime + p-code interpreter image + reserved interpreter heap
  + reserved interpreter stacks + (optional) DSL heap on top +
  source via UART (with EOT) + runtime data via UART

Each layer carries its own segments (code, data, bss, heap, stack),
either embedded inside the layer's artifact (e.g. pvm.s reserves
eval_stack/call_stack/heap_seg internally) or reserved by the
loader at a configured high-SRAM address with patches that tell the
runtime where to find them.

See `docs/prd.md`, `docs/architecture.md`, `docs/design.md`,
`docs/plan.md` for the full picture.

## Working agreement

- TDD red/green/refactor for every step.
- Every step ends with `cargo test`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo fmt --check`, and
  `sw-checklist` all green. Fix sw-checklist failures immediately,
  in the same step that introduces them.
- Commit before `agentrail complete`.
- Keep modules under 500 lines; prefer 200-300.
- No vendoring or cache work in Phase 1 (that is Phase 4).

## Saga scope: Phase 0 + Phase 1

This saga covers the survey (Phase 0) and Scenario A end-to-end
(Phase 1). Phases 2-5 are separate sagas opened after Phase 1 is
green.

## Step list

1. **survey-repos**: produce `docs/survey/index.md` plus one
   `docs/survey/<repo>.md` per working repo (apl, basic, forth,
   macrolisp, ocaml, pascal, plsw, smalltalk, snobol4) and one for
   the failing `sw-vibe-coding/tuplet`. Each per-repo doc captures:
   the run-script argv pattern, every `--load-binary` and `--patch`
   it emits, UART payload structure, where heaps and stacks live
   (embedded vs reserved, address, size, growth direction), what's
   built locally vs vendored, and known limits. End with a
   "Schema gaps observed" section listing TOML fields that would
   need to be added/changed for each repo to be expressible. No
   Rust written. This step's commit includes only docs.
2. **revise-schema**: update `docs/design.md` to cover every gap
   surfaced in step 1; do not invent fields not justified by the
   survey. Update `docs/architecture.md` if invariants change. No
   Rust written. Commit includes only docs.
3. **scaffold-cli**: create the binary with `clap` derive,
   subcommand stubs for `run`/`build`/`check`/`graph`/`cache`/
   `vendor`/`doctor`, and an `error.rs` with `thiserror`. Tests:
   `cargo run -- --version` prints the cargo version;
   `sw-launch run` with no scenario fails with E0001-class
   diagnostic; subcommand stubs exit with a clear "not yet
   implemented" message and a non-zero status.
4. **config-types**: add `serde`/`toml` deps; define `Config`
   covering `[project]`, `[targets.cor24]` (with `regions.sram`,
   `regions.ebr_stack`, `regions.mmio`), `[scenarios.<name>]`,
   `[layers.<name>]`, and `[layers.<name>.segments.<sname>]`.
   Strict deserialization. Tests: parse a minimal Scenario A TOML;
   reject unknown keys; honor `schema_version=1`; target regions
   parse with hex bounds.
5. **scenario-validate**: implement `validate.rs` covering rules
   E0001..E0016 from `docs/design.md`. Wire `sw-launch check
   <scenario>` to run validators and exit non-zero on first error.
   Tests: valid Scenario A passes; missing layer reference,
   kind/load mismatch, missing entry, missing halt condition,
   segment outside SRAM, segment touching EBR, segment touching
   MMIO, embedded segment with missing symbol, non-embedded
   segment without `load.address`, overlap of two memory ranges,
   patch using `self.address` outside a segment block each produce
   the right error code.
6. **assembler-tool**: add `tool::Assembler` (calls `cor24-run
   --assemble`), `tool::ListingParser` (resolves `exports.symbols`
   to absolute addresses, including embedded-stack labels). In-
   process memoization so a single `sw-launch run` invokes the
   assembler at most once per layer (no disk cache yet -- Phase 4).
   Tests: build a tiny .s fixture; second call within the same
   process is a memoization hit; listing parser finds known symbol
   offsets including a labeled `eval_stack` reservation.
7. **scenario-a-loadplan**: implement `manifest::LoadPlan` and
   `target::cor24::build_argv` for Scenario A; segment accounting
   so embedded segments contribute ranges to the overlap map but
   no `--load-binary`. Snapshot tests: argv for a fixture without
   embedded segments; argv for a fixture with embedded
   eval_stack/call_stack/heap; both stable byte-for-byte.
8. **scenario-a-runner**: implement `run::execute` (spawn
   `cor24-run`, capture stdout/stderr, enforce timeout) and
   `expect::check_uart` (substring + regex matchers). Wire
   `sw-launch run <scenario>` end-to-end. Integration test that
   builds a fixture echo.s, runs it under the real `cor24-run`,
   asserts UART output. Mark `#[ignore]` only if `cor24-run` is
   missing from PATH.
9. **phase1-status**: update `docs/status.md` to "Phase 1
   complete"; draft Phase 2 saga seed
   (`docs/saga-phase2-plan.md`) describing Scenario B steps
   grounded in the Phase 0 survey; do not start Phase 2.
   Mark this saga `--done`.
