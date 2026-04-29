# Step 1: pcode-tool

Add the `PcodeAssembler` that wraps `pa24r <input.spc> -o
<output.p24>`, mirroring the contract step 007 established for
the COR24 assembler. This step also implements the first non-
local tool resolution kinds (`from_path` and `sibling`) declared
in `docs/design.md` "Tool model" / "Source resolution kinds".

TDD: tests first, then code.

## Inputs

- `docs/design.md` "Tool model" + "Source resolution kinds" --
  authoritative for the Tool kind / source enum shape.
- `docs/survey/pascal.md` -- references `pa24r` as the host-side
  binary and its CLI shape.
- Step 007's `src/tool.rs` -- reuse the `BuildJob` /
  `BuildOutput` types and the cache-key derivation pattern.
- Step 008's `LoadPlan::build` -- consumes whatever this step
  produces.

## What lands

- `src/tool.rs` (or extracted helper file if function count
  forces it) gains:
  - `PcodeAssembler` with the same shape as `Assembler`:
    `new(tool_path)`, `from_path()` searching `$PATH` for
    `pa24r`, `build(&BuildJob) -> Result<BuildOutput>`. Cache
    key = `(canonical tool_path, sha256(input), extra_args)`.
  - The internal command line: `pa24r <input> -o <output_bin>`.
    p-code assembler does NOT emit a listing; `BuildOutput.listing`
    is set to an empty path (or `Utf8PathBuf::new()`); the
    `Listing` parsed for that artifact is `Listing::default()`.
- The `Tool` model in `docs/design.md` lists `from_path` and
  `sibling` as resolution kinds. This step implements the first
  two practical resolutions:
  - `from_path`: search `$PATH`, return the first match.
  - `sibling`: `{ path: relative-to-config-dir, artifact:
    relative-to-that }`. e.g. for pa24r:
    `source = { sibling = "../sw-cor24-pcode", artifact =
    "target/release/pa24r" }`. Resolution: join the relative
    path to the config file's parent dir, append artifact,
    canonicalize.

## Tests (write first)

`tests/pcode_tool_unit.rs` (new):

- `pcode_assembler_memoization_with_no_op_tool`: same shape as
  `tool_unit::build_with_same_input_is_cached_after_first_spawn`,
  using `/usr/bin/true` so the test runs without `pa24r` on
  PATH. Two builds with same input -> spawn_count == 1.
- `pcode_assembler_distinct_inputs_spawn_twice`: parallel to
  `tool_unit::build_with_different_inputs_spawns_twice`.
- `from_path_resolves_or_errors_cleanly`: not asserting `pa24r`
  is present; assert `from_path()` returns `Ok` if present,
  `Err(Cli)` with message including "pa24r" if absent.

`tests/pcode_tool_integration.rs` (new):

- gated on `pa24r` on `$PATH` (`eprintln!` + return otherwise).
- assemble a 5-line `.spc` fixture
  (`tests/fixtures/scenario_b/hello.spc` -- copy from the
  step prompt below); assert the produced `.p24` is non-empty
  and at least 8 bytes (P24 header is 18 bytes per
  docs/survey/ocaml.md notes; even a hello fits well below
  256 bytes).

If creating two new test files pushes the binary count over
sw-checklist's budget, merge the integration test into
`tests/pcode_tool_unit.rs` behind the same PATH gate.

## Fixture

`tests/fixtures/scenario_b/hello.spc` (new):

```
;; Smallest p-code program that prints "HELLO\n" via the VM's
;; PUTC sys-call, then halts. Used by Phase 2 step 4's end-to-
;; end test. Syntax follows sw-cor24-pcode/vm/examples/hello.spc.

PROC main
  PUSH 'H' SYS PUTC
  PUSH 'E' SYS PUTC
  PUSH 'L' SYS PUTC
  PUSH 'L' SYS PUTC
  PUSH 'O' SYS PUTC
  PUSH 10  SYS PUTC
  SYS HALT
END
```

If the actual `.spc` syntax in `sw-cor24-pcode/vm/examples/hello.spc`
differs (e.g. PROC vs PROCEDURE, sys-call names), copy the
exact form that the assembler accepts and update this fixture.
The test asserts behavior (assembles cleanly + runs HELLO); the
literal source text is whatever pa24r is happy with.

## Pre-commit gate

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
markdown-checker -f "**/*.md"
sw-checklist
```

If a sw-checklist failure beyond the 3 inherited ones from
Phase 1 surfaces, fix it in this step. Document any
unavoidable new failure in the commit (same form as step 006's
trade-off note).

## Done when

- `PcodeAssembler` exists, parallel-shape to `Assembler`.
- `from_path` and `sibling` source resolution both work in
  unit tests with stub paths.
- The integration test against real `pa24r` (when present)
  produces a non-empty `.p24`.
- All gates green; new failures (if any) documented.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 2 (listing-symbol-resolution) is a fresh session.
