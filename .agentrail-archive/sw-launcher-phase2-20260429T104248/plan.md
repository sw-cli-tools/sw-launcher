# sw-launcher Phase 2: Scenario B (runtime + binary blob)

This is the saga seed for `sw-launcher-phase2`. Open the saga
with:

```bash
agentrail init --name sw-launcher-phase2 --plan docs/saga-phase2-plan.md
```

## Goal

Make `sw-launch run pcode-hello` work end to end:

1. Build `pvm.bin` from a vendored `pvm.s` (sw-cor24-pcode).
2. Build `hello.p24` from a fixture `.spc` (the smallest p-code
   program that prints "HELLO" via UART).
3. Resolve `pvm`'s `code_ptr:` symbol from the assembled listing.
4. Load `pvm.bin@0` and `hello.p24@0x010000`, patch
   `code_ptr -> 0x010000`, set `--entry 0`.
5. Spawn cor24-run, capture UART, assert it contains "HELLO".

This is **Scenario B** from `docs/design.md`. The shape mirrors
`sw-cor24-pascal/compiler/scripts/run-pascal.sh` (single-unit
variant) and the demo flow in `sw-cor24-pcode/vm/demo.sh`. See
`docs/survey/pascal.md` (sections 1, 2, 3) and
`docs/survey/ocaml.md` (sections 1, 2, 3) for the prior art.

## Why this scenario

After Phase 1, the launcher can do the simplest case (Scenario A:
one image at 0 + UART data). The bigger jumps are:

- **adding a second tool**: the p-code assembler `pa24r` from
  `sw-cor24-pcode`. Phase 1 only knows `cor24-run --assemble`;
  Phase 2 introduces a non-cor24 tool.
- **resolving cross-layer symbols at runtime**: the `code_ptr`
  patch on `pvm.bin` needs to be filled with the load address of
  the *other* layer (`hello.p24@0x010000`). Phase 1 only has
  literal-hex patches.
- **a vendored layer**: `pvm.s` lives in
  `sw-cor24-pcode/vm/pvm.s` rather than in the consuming repo.
  Phase 2 introduces the simplest vendoring shape (sibling-repo
  path, no version pin yet) so Phase 4 can layer the lockfile on
  top.

After Phase 2, Phase 3 (Scenario C) adds the OCaml-style heap-
limit-only patch, UART-after-EOT data, and reserved interpreter
heap segments. Together, B + C cover roughly 70% of what the
real survey repos do.

## Working agreement

Same as Phase 1:

- TDD red/green/refactor for every step.
- Every step ends with the standard pre-commit gate green:
  `cargo test`, `cargo clippy --all-targets --all-features --
  -D warnings`, `cargo fmt --check`, `markdown-checker`,
  `sw-checklist`. The three known structural trade-offs from
  Phase 1 (`validate.rs` File LOC + Function Count, Crate
  Module Count) carry into Phase 2; new failures must be fixed
  immediately in the same step that introduces them.
- Commit before `agentrail complete`. Push after `complete`.
- Keep modules under 500 lines and 7 functions where possible;
  if Phase 2 forces a fourth structural trade-off, document it
  in the step commit.
- No vendoring lockfile or disk-persistent cache yet; that's
  Phase 4.

## Fixture: tiny p-code "hello" program

The Phase 2 test fixture is a `.spc` source small enough to
assemble + run under `cor24-run` in under a second:

```
;; tests/fixtures/scenario_b/hello.spc
;; Smallest p-code program that writes "HELLO\n" via the VM's
;; PUTC sys-call, then halts.

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

(The exact `.spc` syntax follows
`sw-cor24-pcode/vm/examples/hello.spc`. The fixture is a copy or
near-copy of that file, embedded in this repo so tests don't
depend on a sibling clone.)

The vendored `pvm.s` is referenced via a sibling path:
`../sw-cor24-pcode/vm/pvm.s`. Phase 2 step 1 introduces the
schema for sibling sources; Phase 4 adds the lockfile that pins
the sibling's git SHA at sync time.

## Step list

### Step 1 -- pcode-tool

**What lands**

- `src/tool.rs` (or a new sibling) gains a `PcodeAssembler` that
  wraps `pa24r <input.spc> -o <output.p24>`. Same memoization
  contract as `Assembler` (key = tool path + input sha + args).
  Reuse `BuildJob` / `BuildOutput` types where possible. The
  `tool` module's function-count budget may force a split into a
  shared `tool/spawn.rs`-style helper -- if so, document the
  geometry in the commit.
- `Tool` resolution: `[tools.pcode_assembler]` block in
  `sw-launch.toml` with `source = { path | sibling | from_path }`.
  v1.2 schema already defines this in `docs/design.md` "Tool
  model"; Phase 2 step 1 implements the `from_path` and `sibling`
  resolution kinds (`vendor` waits for Phase 4).

**Tests**

- Unit: memoization with a `/usr/bin/true`-style no-op tool
  (mirrors step 007's `tool_unit.rs` pattern).
- Integration: gated on `pa24r` on PATH; assembles a fixture
  `.spc`, asserts `.p24` is produced and non-empty.

**Pre-commit gate**: standard. **Done when**: pcode_assembler in
the `Tool` model resolves and builds; one negative test for
non-existent input file.

**Stop**: step 2 is a fresh session.

### Step 2 -- listing-symbol-resolution

**What lands**

- Make `Listing::resolve` reachable from the manifest layer for
  patches whose target is `<layer>.<symbol>`. In Phase 1 these
  are accepted by config but not resolved.
- `manifest::LoadPlan::build` looks up cross-layer symbols by
  walking `Artifacts.by_layer.<other>.listing` and replaces the
  symbolic patch with a `ResolvedPatch` whose `address` is the
  symbol's offset *plus* the producing layer's load address.
- New error code: E0006 promoted from "shape only" (Phase 1) to
  "fully resolved" (Phase 2). Negative test: misspelled symbol
  name produces E0006 with a "did you mean ..." Levenshtein
  hint over the producing layer's exported symbols.

**Tests**

- Unit: `cross_layer_symbol_resolves_through_listing`: layer A's
  listing has `code_ptr:` at a known offset; layer B's patch
  with `target = "A.code_ptr"` resolves to that offset.
- Negative: typo in symbol name; assert E0006 + Levenshtein hint.
- Integration (gated on `cor24-run` on PATH): assemble a tiny
  test pvm fixture with `code_ptr:` and assert manifest emits
  the right `--patch 0x<addr>=...`.

**Pre-commit gate**: standard. **Done when**: cross-layer
symbol patches resolve to absolute addresses; symbolic patches
in scenario_a's existing fixture still produce 0 patches
(unchanged).

**Stop**: step 3 is a fresh session.

### Step 3 -- pcode-image-layer

**What lands**

- `Layer.kind = "pcode-image"` (already in v1.2 schema; this
  step actually wires it through `manifest::collect_memory`).
  The pre-linked `.p24m` case isn't needed for Phase 2's
  fixture but is trivial to support alongside `pcode`.
- `Layer.kind = "pcode"` invokes `PcodeAssembler` from step 1
  during `assemble_artifacts` (the cli orchestrator), producing
  a `.p24` artifact that goes through the same `LoadPlan`
  pipeline as the assembler-kind layer.

**Tests**

- Unit: `LoadPlan::build` for a two-layer Scenario B (pvm
  assembler-kind + hello pcode-kind) emits two `--load-binary`
  args at the configured addresses, plus the expected `--patch`
  resolved via step 2.
- Snapshot: golden argv for `tests/fixtures/scenario_b/sw-launch.toml`
  using a stub listing with `code_ptr` at a known offset.

**Pre-commit gate**: standard. **Done when**: `cli::run` invokes
both `Assembler` (for pvm) and `PcodeAssembler` (for hello.spc)
during the build phase, in topological order.

**Stop**: step 4 is a fresh session.

### Step 4 -- scenario-b-fixture-and-runner

**What lands**

- `tests/fixtures/scenario_b/sw-launch.toml` -- complete
  Scenario B: pvm at `0x000000`, hello.p24 at `0x010000`,
  `patches = [{ target = "pcode_vm.code_ptr", value = "0x010000" }]`.
- `tests/fixtures/scenario_b/hello.spc` -- the fixture program
  shown in this doc's "Fixture" section.
- A vendored copy or sibling-path reference to `pvm.s`
  (decision: sibling-path with a documented Phase 4 follow-up
  to pin the SHA, keeping Phase 2 narrow).
- `tests/scenario_b.rs` end-to-end test gated on both
  `cor24-run` and `pa24r` on PATH; asserts UART contains
  "HELLO".
- Negative test: deliberately wrong patch value (e.g.
  `0x020000`) makes the run TRAP or produce a different UART
  string; assert mismatch is reported with the captured UART.

**Pre-commit gate**: standard. **Done when**: real-cor24-run
end-to-end test passes; `cor24-run --load-binary pvm.bin@0
--load-binary hello.p24@0x010000 --patch ...=0x010000 --entry 0`
prints HELLO when invoked manually too.

**Stop**: step 5 is a fresh session.

### Step 5 -- phase2-status

**What lands**

- `docs/status.md` updated: Phase 2 done with the date.
- `docs/saga-phase3-plan.md` drafted (Scenario C seed).
- README.md status line updated.
- `agentrail complete --done`.

**Pre-commit gate**: standard. **Done when**: gates green; the
Phase 3 saga seed cites `docs/survey/ocaml.md` and
`docs/survey/tuplet.md` for the heap-limit-only patch pattern.

**Stop**: Phase 3 is a fresh saga.

## Survey citations

- **Pascal single-unit shape**:
  `docs/survey/pascal.md` sections 2 (Memory loads) and 3
  (Patches) -- the reference for Scenario B's pvm + .p24
  pattern with `code_ptr` patch.
- **OCaml two-binary shape**:
  `docs/survey/ocaml.md` -- reference for the *next* phase's
  symbol-from-sidecar resolution; only relevant to Phase 2 in
  that the symbol-resolution machinery is shared.
- **P-code VM internals**:
  `docs/survey/index.md` table -- pvm carries embedded
  eval/call/heap inside its image; Scenario B's TOML declares
  these as embedded segments so the validator's overlap check
  remains accurate.
- **link24 / multi-module**: out of scope; that's Phase 3+ for
  snobol4-style composite images.

## What's intentionally deferred

- **Disk-persistent cache** (Phase 4 step 1): Phase 2's
  memoization remains in-process. Two consecutive `sw-launch
  run pcode-hello` invocations re-assemble pvm.bin.
- **Vendor lockfile** (Phase 4 step 2): Phase 2 uses a sibling
  path; the SHA isn't pinned anywhere.
- **Heap-limit-only patches** (Phase 3): tuplet's
  `heap_limit -> 0x03F000` pattern is Scenario C, not B.
- **Composite images** (Phase 3 step k): snobol4-style four-
  module link24 composition is later.
- **Resident-shell run mode** (Phase 5): monitor + sws.

## Risks and mitigations

- **`pa24r` not on PATH**: integration tests skip with a clear
  one-line note (matching Phase 1's `cor24-run` gate).
- **pvm.s drifts**: the cross-layer symbol resolution in step 2
  reads the listing fresh each build, so a renamed `code_ptr`
  surfaces as E0006 with the available-symbols list. The
  documented "did you mean" hint makes this debuggable.
- **Sibling path resolves to absent directory**: tool resolution
  in step 1 returns a clear error pointing at the configured
  path; Phase 4 adds the lockfile that turns this into a
  pre-flight check.
- **Function/module count creep**: introducing `PcodeAssembler`
  may push `tool.rs` over its 7-fn cap. If so, accept the
  trade-off and document; do not split into `tool/` directory
  (would push Crate Module Count further over). A future
  refactor extracting `Tool` trait + impls into a sub-crate is
  recorded as Phase 5+ work.
