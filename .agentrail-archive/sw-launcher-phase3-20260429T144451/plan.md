# sw-launcher Phase 3: Scenario C (nested interpreter)

This is the saga seed for `sw-launcher-phase3`. Open with:

```bash
agentrail init --name sw-launcher-phase3 --plan docs/saga-phase3-plan.md
```

## Goal

Make `sw-launch run nested-demo` work end to end against the
sw-cor24-ocaml shape:

1. Build pvm.bin from vendored pvm.s (Phase 2 step 1 already
   handles this via `kind = "assembler"`).
2. Build `ocaml.p24m` -- the OCaml interpreter compiled from
   Pascal to p-code, then linked. (Phase 3 step 1 adds
   `ToolKind::PcodeLinker` so `sw-launch` invokes `p24-load`
   directly rather than the test fixture pre-linking.)
3. Reserve a non-embedded heap segment between pvm and
   ocaml.p24m, with `heap_limit` patched to its end (the OCaml
   runtime's only heap-bound symbol).
4. Compose the UART payload as `<source>.ml + EOT (0x04) +
   <runtime stdin>`.
5. Run cor24-run, capture UART, assert the OCaml interpreter
   evaluated the source and printed expected output.

This is **Scenario C** from `docs/design.md`. The shape mirrors
`sw-cor24-ocaml/scripts/run-ocaml.sh` (single-file mode).

## Why this scenario

Phase 2 closes the runtime + p-code-app + cross-layer-patch
loop. Phase 3 adds three orthogonal pieces:

- **Reserved (non-embedded) memory segments**: a heap claimed
  by the launcher rather than statically allocated inside any
  artifact. Validation (E0011..E0017) already understands this;
  the LoadPlan needs to emit zero-fill metadata so the
  emulator backend knows the range is reserved.
- **Heap-limit-only patches**: the OCaml/tuplet pattern --
  only the heap *ceiling* is patched (`heap_limit -> 0x03F000`);
  the floor is a static label inside the runtime. Schema gap
  B3 in `docs/survey/schema-gaps.md`.
- **UART chunk composition**: `<source> + EOT (0x04) + <stdin>`.
  The current Phase 1 emitter concatenates UART chunks in order
  with optional terminator bytes; Phase 3 verifies the
  byte-for-byte payload against the OCaml runtime's
  expectations.

After Phase 3, Phase 4 adds disk-persistent caching and the
vendor lockfile; Phase 5 stubs multi-target backends.

## Working agreement

Same as Phase 1 + Phase 2:

- TDD red/green/refactor for every step.
- Standard pre-commit gate green at every step's commit. The 7
  inherited sw-checklist trade-offs from Phase 2 carry over;
  new failures must be either fixed in the same step or
  documented as the same root constraint conflict.
- Commit before `agentrail complete`. Push after `complete`
  (then again after the finalize commit).
- Module count is at 8, max 7. Phase 3 should not add new
  source modules; if a step needs more code, extend an existing
  module and accept the growing LOC failures, OR (preferred)
  begin Phase 5's sub-crate extraction for the offending
  module.

## Required ground truth (probe before step 1)

Before starting step 1, confirm the OCaml interpreter's heap
geometry by running its own scripts:

```bash
cd ~/github/sw-embed/sw-cor24-ocaml
./scripts/build.sh          # builds build/ocaml.p24m + pvm.lst
cat build/code_ptr_addr.txt build/heap_limit_addr.txt
```

The two `*_addr.txt` files are the resolved offsets pvm needs.
Phase 3 step 2 will introduce `value = "sidecar:<path>"` patch
forms (schema gap B1) so sw-launch can read those files
directly without the user passing the values manually.

If `~/github/sw-embed/sw-cor24-ocaml/build/` is missing,
Phase 3 step 4's end-to-end test gates skip cleanly.

## Step list

### Step 1 -- pcode-linker

**What lands**

- `ToolKind::PcodeLinker` added to `src/tool.rs`. argv:
  `p24-load <input> -o <output> [--load-addr <addr>]`. Cache
  key includes `extra_args` (already handled).
- `assemble_artifacts` in `cli.rs` extends the `pcode` arm:
  pa24r .spc -> .p24, then p24-load --load-addr <load.address>
  -> .p24m. Records the .p24m as the layer's artifact.
  Lazy-initialize the linker tool the first time a pcode layer
  needs linking.
- Tests: pcode-linker memoization (no-op tool); end-to-end
  rebuild of the Phase 2 hello.spc -> hello.p24m loop using
  sw-launch's own pcode dispatch (drops the test-fixture
  pre-link).

**Done when**

- `sw-launch build pcode-hello` (Phase 2's fixture, switched to
  kind = "pcode") produces a .p24m that pvm runs successfully.
- Phase 2's test still passes; new direct-pcode test passes
  when both pa24r AND p24-load are findable.

### Step 2 -- sidecar patch values + heap-limit-only patch

**What lands**

- `value = "sidecar:<path>"` patch form (schema gap B1). The
  manifest reads the file at `<path>` (relative to config_dir),
  parses a single hex literal (with or without `0x` prefix),
  uses it as the patch value. Lockfile recording is Phase 4.
- Validate E0019 (sidecar staleness): warn when the sidecar's
  mtime is older than the producing artifact's mtime.
- New cross-layer patch shape: target may be a literal hex
  patched directly without symbol resolution -- already
  supported in Phase 1; Phase 3 just exercises it for the
  heap-limit pattern.
- Three new manifest unit tests:
  - sidecar resolves to literal hex value
  - missing sidecar produces a clear E0019 message
  - sidecar with non-hex contents produces a useful error

**Done when**

- A scenario with `value = "sidecar:vendor/sw-pcode/v0.1.0/build/heap_limit_addr.txt"`
  resolves at LoadPlan::build time.

### Step 3 -- reserved heap segments emit zero-fill metadata

**What lands**

- `LoadPlan.segments` for non-embedded `kind = "heap"` /
  `"stack"` / `"bss"` segments contributes a `MemoryLoad`-like
  entry that tells the emulator backend to reserve the range.
  cor24-run accepts pre-zeroed memory by default, so the
  argv emitter doesn't need a new flag -- but the validator
  must still see the segment in its overlap accounting (E0003).
- LoadPlan::cor24_argv unchanged; reserved segments contribute
  to overlap checks but not to argv output (the emulator
  zero-initializes any address not covered by --load-binary).
- Two new manifest unit tests:
  - reserved heap segment contributes to LoadPlan.segments but
    not to memory_loads
  - reserved heap segment overlapping with a memory_load fires
    E0003 from the validator

**Done when**

- Existing Phase 2 tests still pass.
- New tests for the reserved-segment overlap check pass.

### Step 4 -- uart chunk composition (EOT + stdin)

**What lands**

- Manifest's UART layer collection groups multiple uart-method
  layers in their declared order. Each chunk's terminator
  (EOT/ETX/EOF/none) appends after the chunk's bytes. The
  resulting `--uart-input` argv slot is the byte concatenation.
- Phase 1's logic already does this for one chunk + one
  terminator; Phase 3 step 4 just adds tests verifying the
  multi-chunk case (`source.ml + 0x04 + stdin.txt`).
- Three new manifest unit tests:
  - two UART chunks in declared order produce
    `bytes_a + term_a + bytes_b + term_b`
  - terminator = "none" appends nothing
  - empty input file produces just the terminator

**Done when**

- The UART payload byte string for a multi-chunk scenario
  matches a golden hex dump.

### Step 5 -- scenario-c-fixture-and-runner

**What lands**

- `tests/fixtures/scenario_c/sw-launch.toml`: full nested-
  interpreter scenario.
  ```toml
  [layers.pcode_vm]      kind = "assembler" input = "../../.../pvm.s"
                         exports.symbols = ["code_ptr", "heap_limit"]
  [layers.ocaml_interp]  kind = "binary"    input = "...build/ocaml.p24m"
  [[layers.ocaml_interp.segments]]
    kind = "heap"  embedded = false  size = "0x02F000"
    [layers.ocaml_interp.segments.load]
      method = "memory" address = "0x010000"
    patches = [
      { target = "pcode_vm.heap_limit", value = "self.end" }
    ]
  patches = [
    { target = "pcode_vm.code_ptr", value = "ocaml_interp.address" }
  ]
  [layers.ocaml_source]  kind = "text"     input = "demo.ml"
    [layers.ocaml_source.load]
      method = "uart"  max_bytes = 16384  terminator = "EOT"
  ```
- `tests/fixtures/scenario_c/demo.ml`: small OCaml program
  that prints a known string (e.g. `print_int (1+2)` ->
  expected UART output contains "3").
- `tests/scenario_c.rs` two end-to-end tests:
  1. `sw_launch_run_ocaml_demo_against_real_tools`: gated on
     cor24-run + pa24r + p24-load + sw-cor24-ocaml/build/ +
     pvm.s presence. Asserts UART output contains the expected
     evaluation result.
  2. `run_with_oversized_source_fails_with_e0004`: same
     fixture but a deliberately-oversized .ml file (> 16 KiB);
     assert validate fires E0004 before the run.
- The "self.end" segment-internal patch form (Phase 3 step 3
  prep) is exercised here; if step 3 didn't land it, fall
  back to a literal-hex patch.

**Done when**

- Real-tools-present scenario_c test passes (UART contains the
  evaluated result).
- Oversized-source test fires E0004.

### Step 6 -- phase3-status

Standard close-out: status.md updates, draft Phase 4 saga seed
(disk cache + vendor lockfile + doctor + graph), README.md,
`agentrail complete --done`.

## Survey citations

- `docs/survey/ocaml.md` sections 1-5: nested-interpreter shape,
  heap-limit-only patch, UART <source>+EOT+<stdin> composition,
  build-time-resolved sidecar files for `code_ptr_addr.txt` and
  `heap_limit_addr.txt`.
- `docs/survey/tuplet.md` sections 2-4: same pattern with a
  binary memory image at 0x080000 (Phase 3 doesn't cover the
  binary-image case; it's Phase 4-ish).
- `docs/survey/schema-gaps.md` B1, B3, A1: sidecar values,
  limit-only heap, composite images.
- `docs/heap-analysis.md` ocaml row: working OCaml heap is
  ~252 KiB pre-GC, target <= 64 KiB post-GC. Phase 3 fixture
  doesn't enforce a budget; Phase 5 will add the heap-budget
  check against the chosen `memory_profile`.

## Risks and mitigations

- **OCaml interpreter changes its heap geometry**: the sidecar
  files are read fresh each build, so a renamed `heap_limit`
  surfaces as a clear E0006 / E0019 error. Tests gate on the
  sw-cor24-ocaml repo being present and built.
- **`p24-load --load-addr` semantics drift**: Phase 3 step 1
  adds an integration test that exercises the linker; if its
  argv shape changes, the test fails loudly.
- **UART chunk encoding edge cases**: the OCaml runtime is
  picky about EOT placement (per
  `docs/survey/ocaml.md#stdin-and-getc`). Step 4's tests use
  a small fixture that's known to work in the upstream repo.

## Function/module-count budget

Phase 3 must not add a new top-level source module. Crate
module count is at 8 (cap 7). The new `ToolKind::PcodeLinker`
extends an enum (no fn add). The sidecar patch reader and
reserved-segment emitter extend `manifest.rs`; expect 1-2
new fns there, pushing the count further over its already-
documented overage. New LOC failures are likely; document
them as the same root constraint.

If a step's requirements *need* more decomposition than the
caps allow, the right move is to begin the Phase 5 sub-crate
extraction (`sw-launcher-validate`, `sw-launcher-tool`, or
`sw-launcher-manifest`) **in that step** rather than papering
over with another documented failure. Document the decision
in the step commit.
