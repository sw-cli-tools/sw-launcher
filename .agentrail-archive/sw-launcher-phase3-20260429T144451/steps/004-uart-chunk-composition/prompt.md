# Step 4: uart-chunk-composition

Multi-chunk UART payload composition. Phase 1 already concatenates
chunks in declared order with optional terminator bytes; Phase 3
step 4 verifies the multi-chunk case (`source.ml + 0x04 + stdin`)
matches the OCaml runtime's expectations byte-for-byte.

## Inputs

- `docs/survey/ocaml.md` section "UART payload structure" -- the
  authoritative byte-stream definition.
- `src/manifest.rs::collect_uart` -- existing single-chunk logic.
- `src/manifest.rs::LoadPlan::cor24_argv` -- existing chunk
  concatenation with terminator bytes.

## What lands

No production-code changes expected -- the existing logic should
handle multi-chunk cases. This step's value is *test coverage*:
we add tests that lock in the byte-for-byte payload shape so
future changes don't drift.

If a test reveals a bug (e.g. wrong terminator order, missing
chunk separator), fix it in this step and document the fix in
the commit.

## Tests

tests/manifest_unit.rs (+3 tests)
  - two_uart_chunks_in_declared_order_concatenate_with_terminators:
      chunk A: bytes "let x = 1 + 2;\n", terminator EOT (0x04)
      chunk B: bytes "exit 0\n", terminator "none"
      payload = "let x = 1 + 2;\n\x04exit 0\n"
      Verify byte-for-byte against a hex-literal expected value.

  - terminator_none_appends_no_byte:
      chunk A: bytes "x", terminator "none"
      payload = "x"

  - empty_input_file_just_terminator:
      chunk A: empty file, terminator EOT
      payload = "\x04"

## Pre-commit gate

Standard.

## Done when

- 3 new manifest unit tests pass.
- Existing tests unchanged.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 5 (scenario-c-fixture-and-runner) is a fresh session.
