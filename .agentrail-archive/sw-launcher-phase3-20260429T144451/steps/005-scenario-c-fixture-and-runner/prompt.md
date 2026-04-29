# Step 5: scenario-c-fixture-and-runner

The full nested-interpreter scenario: pvm + ocaml.p24m +
reserved heap + ocaml source via UART. Mirrors the pattern in
sw-cor24-ocaml/scripts/run-ocaml.sh.

## Inputs

- Steps 1-4 of this saga (PcodeLinker, sidecar values, reserved
  segments, UART composition).
- `docs/survey/ocaml.md` -- complete shape reference.
- `~/github/sw-embed/sw-cor24-ocaml/build/` (must be built by
  `just build` in that repo before this test can run).

## What lands

tests/fixtures/scenario_c/sw-launch.toml
  - layer pcode_vm: kind = assembler, input = pvm.s,
    exports.symbols = ["code_ptr", "heap_limit", "heap_seg",
                        "eval_stack", "call_stack"]
    load.address = 0x000000
  - layer ocaml_interp: kind = pcode-image, input = ocaml.p24m,
    load.address = 0x040000 (matching ocaml repo's build).
    patches = [{ target = "pcode_vm.code_ptr", value =
                  "ocaml_interp.address" }]
    Plus a reserved heap segment with patches that point
    pcode_vm.heap_limit at the segment's end.
  - layer ocaml_source: kind = text, input = demo.ml,
    load.method = uart, terminator = EOT, max_bytes = 16384.

  Whether to use sidecar:* patch values or literal hex depends
  on step 2's outcome. If step 2 landed: use
  `value = "sidecar:<ocaml-build>/code_ptr_addr.txt"` etc.
  Otherwise: parse pvm.lst at test-fixture-prep time and write
  literal hex.

tests/fixtures/scenario_c/demo.ml
  Tiny OCaml program with deterministic output:
    print_int (1 + 2)
    print_newline ()
  Expected UART tail: "3\n".

tests/scenario_c.rs (new, 2 tests)
  - sw_launch_run_ocaml_demo_against_real_tools:
      gated on cor24-run + pa24r + p24-load + pvm.s + ocaml
      build/ directory presence;
      runs `sw-launch run nested-demo --config <fixture>`;
      asserts UART output contains "3\n" (or the documented
      OCaml interpreter prelude line + "3").

  - run_with_oversized_source_fails_with_e0004:
      same fixture but with a deliberately-oversized .ml file
      (> 16 KiB);
      assert validate fires E0004 before any tool runs.

## Pre-commit gate

Standard.

## Done when

- Real-tools-present scenario_c test passes (UART contains the
  evaluation result).
- Oversized-source test fires E0004.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 6 (phase3-status) is a fresh session.
