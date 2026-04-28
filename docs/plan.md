# sw-launcher Plan

The plan is sequenced for "smallest useful tool first." Each phase
ends with a green pre-commit gate (tests, clippy, fmt,
sw-checklist) and a working subset of `sw-launch`. Agentrail steps
in the saga track the day-by-day work.

## Phase 0 -- survey existing layouts (informs all later phases)

Before writing schema-tied code, capture the *real* memory layouts
used today across the corpus of working repos and the failing one:

- working: `sw-embed/sw-cor24-{apl,basic,forth,macrolisp,ocaml,
  pascal,plsw,smalltalk,snobol4}`
- failing canonical: `sw-vibe-coding/tuplet`

For each repo, record in a single file under `docs/survey/`:

- the run script(s) and the exact `cor24-run` argv they emit
- every `--load-binary <path>@<addr>` and the rationale
- every `--patch <addr>=<value>` and the symbol it points to
- whether UART carries source, data, both, or none
- where the heap and stack(s) actually live (embedded? reserved?
  grows up or down? size? how was the address chosen?)
- which build artifacts come from this repo and which are
  vendored, with version pin if any
- known limits: max source size, max image size, max cycles

Output: `docs/survey/index.md` with a comparison table, plus one
`docs/survey/<repo>.md` per repo. Conclude with a section
"Schema gaps observed" that lists every TOML field we'd need to
add or change to express each repo without loss. The schema in
`docs/design.md` then gets revised to cover those gaps before
Phase 1 step 2.

This step writes no Rust. It writes prose and one table. Treat
`tuplet` specially: write a hypothesis about why its current
script pattern fails, and what `sw-launch` validation rule would
have caught it (likely a heap/data overlap or a missing patch).

## Phase 1 -- skeleton + Scenario A (assembled program at zero, UART)

Goal: `sw-launch run echo` runs an assembled `.s` against `cor24-run`
with UART input and asserts on UART output.

1. Scaffold the binary: `clap`, `--version`, `run`/`check`/`build`
   stubs, README pointer, error type. Test: `cargo run -- --version`.
2. Define `Config` types for `[project]`, `[targets.cor24]` (with
   `regions.sram`, `regions.ebr_stack`, `regions.mmio`),
   `[scenarios.<name>]`, `[layers.<name>]`, and
   `[layers.<name>.segments.<sname>]`. Tests: parse a minimal
   Scenario A TOML; reject unknown keys; honor `schema_version=1`;
   target regions parse with hex bounds.
3. Implement `validate::layer_load_compat`, `validate::scenarios`,
   `validate::regions`, and `validate::overlap` (segment-aware).
   Tests: valid Scenario A passes; missing layer reference,
   kind/load mismatch, missing entry, segment outside SRAM, segment
   touching EBR, segment touching MMIO each produce the right error
   code (E0001..E0016 in design.md).
4. Implement `tool::Assembler` (a thin `cor24-run --assemble` wrapper
   producing `.bin` + `.lst`) and `tool::ListingParser` that resolves
   `exports.symbols` to absolute addresses. Tests: build a tiny .s
   fixture; cache miss invokes once, cache hit invokes zero times;
   listing parser finds known symbol offsets including embedded-stack
   labels.
5. Implement `manifest::LoadPlan` and `target::cor24::build_argv` for
   Scenario A, plus segment accounting (embedded segments contribute
   ranges to the overlap map but no `--load-binary`). Test: golden
   snapshot of the argv given a fixture; second snapshot showing a
   layer with embedded stack/heap segments.
6. Implement `run::execute` and `expect::check_uart`. Test: end-to-end
   `sw-launch run echo` against a fixture echo.s + expected output.

Exit criteria: `cargo test`, clippy `-D warnings`, fmt, sw-checklist
all green; running phase 1 twice is one cold + one hot run; integration
test for Scenario A passes against the real `cor24-run`.

## Phase 2 -- Scenario B (runtime + binary blob, code_ptr patch, embedded stacks)

Goal: `sw-launch run pcode-hello` builds `pvm.bin` from a vendored
`pvm.s`, builds `hello.p24` from a fixture `hello.spc`, loads them at
0 and 0x010000, patches `code_ptr`, accounts for `pvm.bin`'s
embedded eval/call stacks and heap (so a future Scenario C heap can
be placed above without colliding), runs, asserts UART output.

7. Vendor manifest reading + `vendor:<pkg>@<ver>` source resolver
   (local-path stub). Test: a layer with `source = "vendor:..."`
   resolves to a path on disk.
8. Symbol export: parse a layer's listing for `<symbol>:` and record
   the absolute address. Test: pvm.lst fixture yields known
   `code_ptr` value.
9. `tool::PcodeAssembler` (`pa24r`) wrapper. Test: build a tiny .spc
   fixture; cache hit on rerun.
10. Multi-layer LoadPlan: ordered `--load-binary X@addr` arguments,
    `--patch <addr>=<value>` for symbolic patches. Test: golden argv
    snapshot for Scenario B.
11. Memory overlap validator. Tests: overlap of two memory layers
    fails with the expected error and span; non-overlap succeeds.
12. End-to-end Scenario B integration test against real `cor24-run`.

Exit criteria: same gates green; Scenario B fixture is the smallest
plausible "VM + bytecode" example we can build.

## Phase 3 -- Scenario C (nested interpreter + reserved heap/stack + UART source/data)

Goal: `sw-launch run nested-demo` loads pvm + a pre-built `.p24m`
interpreter, *reserves* the interpreter's value heap and eval stack
in high SRAM and patches the runtime to point at them, feeds source
via UART (with EOT), then appends runtime data via UART, asserts
output. Adds the optional fourth layer (DSL on top of interpreter)
with its own heap, demonstrating the full segment-collision story.

13. UART layer: ordered list of UART chunks, each with terminator
    handling. Test: the assembled `--uart-input` payload byte-for-byte
    matches a golden fixture for source + EOT + data.
14. UART size validator. Tests: oversize source fails with the
    expected error; right-sized passes.
15. `pcode-image` layer (no build, just hash + load). Test: cache key
    stable across runs of the same .p24m.
16. Reserved (`embedded = false`) segments: `heap`, `stack`, `bss`.
    Allocate range, zero-fill on demand, expand `self.address` /
    `self.end` / `self.size` in patches. Tests: two interpreters
    each with their own heap; collision detected; non-collision
    layout passes.
17. Profiles (`demo`, `test`); CLI override of timeout / max_cycles.
    Tests: profile selection picks correct values.
18. End-to-end Scenario C integration test against real `cor24-run`,
    using a small pre-built interpreter fixture (vendored, not built
    from source in this repo). Includes a four-layer variant that
    adds a DSL heap above the interpreter heap.

Exit criteria: gates green; Scenario C demonstrates source-via-UART
and binary-via-memory in the same scenario.

## Phase 4 -- caching, vendor sync, doctor, graph

18. Content-addressed cache directory layout under
    `~/.cache/sw-launch/`. Test: deterministic across two runs;
    `cache list` reflects state; `cache explain` shows hit/miss per
    layer.
19. `sw-launch.lock` writer + reader; `vendor sync` (local-path
    stub) computes lock entries; `vendor status` compares TOML vs
    lock vs disk. Tests for each diff case.
20. `doctor` subcommand: verify `cor24-run`, `pa24r`, `pl24r` are on
    PATH or explicitly configured. Test: `--config` pointing at a
    file with a missing tool fails with a usable hint.
21. `graph` subcommand: text + `--json` output of the layer DAG.
    Snapshot test.

Exit criteria: gates green; running any of the three scenarios twice
is a 100% cache hit on the second run.

## Phase 5 (deferred) -- multi-target stubs

- `target::ibm1130` / `target::rca1802` / `target::riscv32` skeletons
  that fail with "not implemented" but accept the schema.
- Doc updates explaining how to add a backend.
- Lockfile + cache pre-existing-keyed-by-target so a single config
  can hold scenarios for multiple targets without churn.

No code from phase 5 is in scope until phases 1-4 are green.

## Risks and how each phase mitigates

- **Tool-path drift across machines.** Phase 4's `doctor` and
  `vendor sync` make this explicit; phase 1 tolerates it via
  `[tools.*]` overrides.
- **Cache poisoning from non-deterministic builds.** Phase 1 hashes
  inputs *and* tool versions; phase 4 records full provenance.
- **Schema churn.** `schema_version = 1` everywhere; bumping is a
  visible step, not a silent migration.
- **Test brittleness against real `cor24-run`.** Each scenario keeps
  the fixture's expected UART output minimal and stable; we use
  `uart_contains` rather than full equality.
