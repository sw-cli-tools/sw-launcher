# Step 4: scenario-b-fixture-and-runner

Land the canonical Scenario B fixture and the end-to-end
integration test. Closes the practical loop that Phase 1 closed
for Scenario A.

## Inputs

- Steps 1-3 of this saga (PcodeAssembler, cross-layer symbol
  resolution, pcode-kind layer dispatch).
- The Scenario B sketch in `docs/saga-phase2-plan.md`.
- `docs/survey/pascal.md` for the cor24-run argv shape this
  test should reproduce.

## What lands

- `tests/fixtures/scenario_b/sw-launch.toml`: complete
  Scenario B with two layers:
  ```toml
  [layers.pcode_vm]
  kind = "assembler"
  input = "../../../../sw-cor24-pcode/vm/pvm.s"  # sibling path
  artifact = "pvm.bin"
  exports = { symbols = ["code_ptr"] }
  [layers.pcode_vm.load]
  method = "memory"
  address = "0x000000"

  [layers.pcode_app]
  kind = "pcode"
  input = "hello.spc"
  artifact = "hello.p24"
  patches = [
    { target = "pcode_vm.code_ptr", value = "0x010000" }
  ]
  [layers.pcode_app.load]
  method = "memory"
  address = "0x010000"

  [scenarios.pcode-hello]
  target = "cor24"
  layers = ["pcode_vm", "pcode_app"]
  entry = "0x000000"
  [scenarios.pcode-hello.run]
  mode = "batch"
  timeout_ms = 5000
  max_cycles = 5_000_000
  halt_on = "monitor-exit"
  [scenarios.pcode-hello.expect]
  uart_contains = ["HELLO"]
  ```

- `tests/fixtures/scenario_b/hello.spc`: copy or near-copy of
  `sw-cor24-pcode/vm/examples/hello.spc` shipped from step 1.

- `tests/scenario_b.rs` end-to-end test: same shape as
  `tests/scenario_a.rs`. Two tests:
  1. `sw_launch_run_pcode_hello_against_real_tools`:
     gated on both `cor24-run` AND `pa24r` on PATH; spawns
     `sw-launch run pcode-hello`; asserts success +
     stdout contains "HELLO".
  2. `run_with_misaligned_patch_fails_visibly`:
     mutate the fixture so `code_ptr` is patched to a wrong
     address (e.g. 0x020000); assert run exits non-zero with
     a recognizable failure mode (TRAP message or empty UART
     + exit_code mismatch). The exact assertion is "stderr
     contains 'expectation mismatch' OR cor24-run TRAPs"; in
     either case the test confirms the failure surfaces.

If the sibling path `../sw-cor24-pcode` is not present on the
test machine (CI scenarios), both integration tests
`eprintln!` and return cleanly, mirroring Phase 1's gating
pattern.

## Pre-commit gate

Standard.

## Done when

- Real-tools-present scenario_b end-to-end test passes
  (HELLO observed in UART).
- Misaligned-patch test reports a clear failure on stderr.
- Manual smoke: `sw-launch run pcode-hello` from the fixture
  dir prints HELLO and exits 0.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 5 (phase2-status) is a fresh session.
