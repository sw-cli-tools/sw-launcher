# Step 1: pcode-linker

Add `ToolKind::PcodeLinker` so `sw-launch` invokes `p24-load`
directly. Phase 2's scenario_b test pre-linked via the test
fixture; this step makes that built-in.

TDD: tests first.

## Inputs

- `tests/scenario_b.rs` -- the existing test that does the
  pre-link manually. Phase 3 step 1 makes the launcher do that
  step itself; the test gets simpler (no `p24-load` invocation
  in the test body, just `sw-launch run` with kind = "pcode").
- `src/tool.rs` -- existing `Tool` + `ToolKind` enum. Adding
  `Pcode` was step 1 of Phase 2; this step adds `PcodeLinker`.
- `src/cli.rs` -- `assemble_artifacts` pcode arm currently
  produces `<layer>.p24`; this step extends it to also run
  `p24-load --load-addr <load.address>` to produce
  `<layer>.p24m`.

## What lands

src/tool.rs
  - `ToolKind::PcodeLinker` variant. argv shape in `Tool::build`:
    `<input.p24> -o <output.p24m> [extra_args]`. The `--load-addr
    <addr>` is passed via `BuildJob.extra_args` so the cache key
    distinguishes runs at different load addresses.

src/cli.rs
  - `assemble_artifacts` pcode arm:
      pa24r .spc -> <layer_dir>/<layer>.p24      (existing)
      p24-load .p24 --load-addr <addr> -> .p24m  (new)
      record .p24m as the layer's artifact.
  - Lazy-init the `linker: Option<Tool>` alongside the existing
    `pcode: Option<Tool>`. Both are only constructed if a
    pcode-kind layer is encountered.

## Tests

tests/tool_unit.rs (+1 test)
  - pcode_linker_argv_includes_load_addr_in_extra_args:
    constructs a Tool with kind = PcodeLinker, builds with
    `extra_args = ["--load-addr", "0x010000"]`, and uses
    `/usr/bin/true` as the no-op binary; asserts spawn_count
    increments and the cache key includes the args.

tests/scenario_b.rs (refactor)
  - Drop the test's own `p24-load` invocation. Switch the
    fixture's pcode_app layer from `kind = "pcode-image"`
    (with input pointing at a pre-linked .p24m) to
    `kind = "pcode"` (with input pointing at hello.spc). Phase
    2's behavior + assertions stay the same.
  - Update skip_unless_ready: still gates on cor24-run, pa24r,
    p24-load, pvm.s presence -- but now sw-launch handles the
    pa24r + p24-load chain itself.

## Pre-commit gate

Standard.

## Done when

- New tool unit test passes.
- tests/scenario_b.rs still passes (UART output unchanged); the
  test body is shorter because it no longer invokes p24-load.
- `sw-launch build pcode-hello` (Phase 2 fixture, kind="pcode")
  produces a .p24m suitable for pvm to run.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 2 (sidecar-patch-values) is a fresh session.
