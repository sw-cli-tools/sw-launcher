# Survey: sw-cor24-pascal

p24p is a Pascal compiler written in C, cross-compiled by tc24r into a
single COR24 assembly file (compiler/p24p.s, ~78K lines) that runs on
the COR24 emulator. The compiler reads .pas source over UART, emits
.spc p-code assembler text, which is then linked with a hand-written
runtime (runtime/runtime.spc) by pl24r, assembled by pa24r into a
relocatable .p24, post-processed by a Python relocator
(scripts/relocate_p24.py) to load address 0x010000, and finally run on
pvm.s -- a separate p-code VM that itself executes inside cor24-run.
The repo has two distinct memory-load shapes: single-unit (binary blob
relocated to 0x010000 + a code_ptr patch) and multi-unit (a v2 .p24m
image loaded by p24-load with no relocator step, code_ptr patched the
same way). All p-code tools live in the sibling sw-cor24-pcode repo;
nothing is vendored here.

## 1. Run scripts

Top-level scripts/: build.sh (cargo-builds sibling pa24r/pl24r/p24-load)
and relocate_p24.py.

compiler/justfile (justfile:39-72) defines `build` (regen p24p.s via
tc24r), `run <file>` (feed Pascal over UART), `test`, `test-e2e`,
`demo`, `demo-all`, `demo-led`, `run-unit`, `build-runtime-unit`.

compiler/scripts/ runnables:
- run-pascal.sh (66)       single-unit pipeline
- run-pascal-unit.sh (78)  single-file unit-mode pipeline
- run-multi-unit.sh (103)  multi-file unit-mode pipeline
- compile-unit.sh (36)     unit -> .spc + .spi
- demo.sh (115)            visible single-unit pipeline
- demo-led.sh (88)         LED on/off with hardware dump
- demo-multi-unit.sh (113) multi-unit walkthrough (asm step blocked)
- demo-all.sh, test-all.sh regression batches

compiler/tests/run_pipeline.sh exercises the codegen-test C program
with pasm/pvm, separate from the .pas end-to-end suite.

## 2. Memory loads

Two distinct shapes, both target a load address of `0x010000`:

A. Single-unit (run-pascal.sh:59-65, demo.sh:91-95)

    cor24-run --run pvm.s
      --load-binary <name>.bin@0x010000
      --load-binary code_ptr.bin@${CODE_PTR_ADDR}
      --terminal --speed 0 -n 50000000

   `<name>.bin` is the post-relocator output of relocate_p24.py
   (scripts/relocate_p24.py:26 strips the 18-byte .p24 header and
   patches absolute push-data operands). `code_ptr.bin` is exactly the
   3 bytes `\x00\x00\x01` (run-pascal.sh:56). `${CODE_PTR_ADDR}` is
   resolved at run time by re-running pvm.s with `-e code_ptr` to read
   the symbol's address (run-pascal.sh:26-27).

B. Multi-unit / unit-mode (run-multi-unit.sh:97-100, run-pascal-unit.sh:71-77)

    cor24-run
      --load-binary pvm.bin@0
      --load-binary <name>.p24m@0x010000
      --patch 0x${CODE_PTR}=0x010000
      --entry 0 --speed 0 -n N --terminal

   pvm.s is pre-assembled in a separate cor24-run --assemble step
   (run-multi-unit.sh:88-89) into pvm.bin + pvm.lst; CODE_PTR is
   parsed from the .lst with grep+awk (run-multi-unit.sh:90). The
   .p24m image is the linker output from p24-load and contains its
   own header that the VM's boot code recognizes (pvm.s:100-176).
   No relocate_p24.py step is needed -- p24-load was invoked with
   `--load-addr 0x010000` (run-multi-unit.sh:85).

Loaded segments inside pvm.s itself (pvm.s:3571,3614,3668,3800,3858):
code_ptr, globals_seg (1536 B), call_stack (4096 B), eval_stack
(1536 B), heap_seg.

## 3. Patches

Single-unit: code_ptr is patched by writing little-endian 0x010000 to
the address resolved from `pvm.s -e code_ptr`, via a 3-byte file
loaded with `--load-binary` (run-pascal.sh:55-61, demo-led.sh:28).
relocate_p24.py rewrites every `push <data_ref>` operand whose
immediate falls in the data segment, adding load_addr in place;
walks opcode lengths inline (relocate_p24.py:13-25).

Multi-unit: code_ptr is patched via `--patch 0x${CODE_PTR}=0x010000`
instead of a load-binary blob (run-multi-unit.sh:99). vm_flags is
adjacent (pvm.s:3574-3579) but unused by any script. No other
patches: p24-load did all internal relocation already.

## 4. UART payload structure

The compiler is fed source via cor24-run's `-u` preloaded-UART flag
because `--terminal` has a ~4 KB buffer cap (CLAUDE.md:124-126,
run-pascal.sh:34-35).

Payload for compile invocations is just `cat <file.pas>` followed by
EOT (`\x04`) -- run-pascal.sh:35, run-pascal-unit.sh:29,
compile-unit.sh:18.

Multi-unit twist (run-multi-unit.sh:46-69, demo-multi-unit.sh:67-77):
the main program's input is *prefixed* with the .spi files of every
already-compiled unit, each wrapped in literal markers:

    ;--- SPI <unit-name> ---
    <contents of unit.spi>
    ;--- END SPI ---

then the actual main.pas, then `\x04`. The compiler scans for those
markers to learn unit interfaces.

Compiler output (also UART) carries multiple region markers the host
sed-extracts (run-pascal.sh:44, run-pascal-unit.sh:38,42-44):
- `.module ... .endmodule` -- single-unit module
- `.unit ... .endunit`     -- unit body
- `;--- SPI --- ... ;--- END SPI ---` -- emitted .spi block when
  compiling a unit
- a trailing `; OK` line as the success sentinel
  (run-pascal.sh:38, compile-unit.sh:21)

Compile budget: --speed 0 -n 50000000 (50M COR24 instructions).
Run budget: same default, override-able as $2 to run-pascal.sh.

For pvm-test usage, `\x04` likewise terminates pvmasm UART input
(CLAUDE.md:108-111, 144).

## 5. Heaps and stacks

VM-side regions in pvm.s (sw-cor24-pcode):
- eval_stack: 1536 B (pvm.s:3800-3801)
- call_stack: 4096 B (pvm.s:3668-3669)
- globals_seg: 1536 B (pvm.s:3614-3615)
- heap_seg: bump-style arena, accessed via syscall 4 (ALLOC) / 5
  (FREE); block header/footer at pvm.s:2869+.

Pascal runtime heap layer (runtime/runtime.spc:419-451 + .pas docs in
runtime/heap.pas, io_state.pas, write_fmt.pas, read.pas):
`_p24p_heap_init` (line 422) zeroes a fixed 16-slot pointer table
`_h_pt` (.global _h_pt 16) plus counters `_h_ac`/`_h_fc`. `_p24p_new`
(line 454) calls sys 4 then records the addr in the next free slot.
`_p24p_dispose`/`_p24p_leak_report` complete the API (.spi:39-43).
No per-program heap sizing in any run script -- VM owns the heap.

Pascal program stacks: programs use VM eval_stack and call_stack;
local frames allocated by `enter`/`leave`, auto-emitted by pvmasm
from `.proc`/`.end` (CLAUDE.md:140-145).

## 6. Build artifacts vs vendored

No `vendor/` dir. All p-code tooling is sibling-relative:

| Tool       | Source                  | Resolved by                      |
|------------|-------------------------|----------------------------------|
| pa24r      | sw-cor24-pcode          | ../sw-cor24-pcode/target/release/pa24r (run-pascal.sh:16) |
| pl24r      | sw-cor24-pcode          | same prefix                      |
| p24-load   | sw-cor24-pcode          | same prefix                      |
| pvm.s      | sw-cor24-pcode          | sw-cor24-pcode/vm/pvm.s          |
| cor24-run  | sw-cor24-emulator       | $PATH                            |
| tc24r      | sw-vibe-coding/tc24r    | $PATH (justfile:1)               |

Per-program build artifacts in /tmp/p24p_$$ (run-pascal.sh:21):
`<name>.spc` (compiler), `<name>_linked.spc` (pl24r), `<name>.p24`
(pa24r, 18-byte header), `<name>.bin` (relocator output, header
stripped), `code_ptr.bin` (3-byte patch). Multi-unit also:
`<name>.p24m` (p24-load image), `pvm.bin`, `pvm.lst`.

Checked-in artifacts: compiler/p24p.s (generated, ~78K L, regen via
`just build`, never hand-edit per CLAUDE.md:115-118);
runtime/runtime.spc (1021 L canonical pl24r flow);
runtime/runtime-unit.spc (1021 L unit-mode flow, `.unit p24p_rt`);
runtime/runtime.spi (55 L interface auto-gen from runtime-unit.spc);
runtime/p24p_rt.p24 (16 B pre-assembled image, `just
build-runtime-unit`, justfile:71-72). The .pas files in runtime/
(checks/heap/io_state/read/write_fmt.pas) document the intended
self-hosted impl but the .spc is canonical (CLAUDE.md:7-12).

## 7. Known limits

- 128 string literals in a packed pool, <=256 symbols, <=128
  procedures, 32 KB source input cap (README.md:51-52).
- ~4 KB cor24-run terminal-buffer cap forces preloaded UART input
  (CLAUDE.md:124-126).
- pvmasm input_buf is 512 B, so hand-written .spc tests must be
  compact, no comments (CLAUDE.md:142-145).
- code_ptr is not at a fixed address; must be resolved per pvm.s
  build via `-e code_ptr` or by parsing pvm.lst
  (CLAUDE.md:128-130, run-pascal.sh:26, run-multi-unit.sh:90).
- Programs are hard-coded to load at 0x010000; relocate_p24.py and
  p24-load both take that as an arg (CLAUDE.md:131-133,
  run-pascal.sh:53,61).
- pa24r requires main to be a unit, and p24-load import resolution
  is incomplete: demo-multi-unit.sh:97-104 explicitly notes
  blocking issues sw-cor24-pcode#7 and #8.
- Single C translation unit, no function pointers, 24-bit integers,
  bump-only malloc inside the compiler itself (CLAUDE.md:152-158).
- Compile budget 50M COR24 instructions per file is enough for the
  current test corpus but not parameterized in any script.

## Schema gaps for this repo

A sw-launcher schema must capture:

1. Two distinct load shapes for the SAME repo: (a) single-unit uses
   `--load-binary <bin>@0x010000` plus a 3-byte `code_ptr.bin@<sym>`
   blob; (b) unit/multi-unit uses `pvm.bin@0` plus
   `<image>.p24m@0x010000` plus `--patch 0x<sym>=0x010000`. Schema
   must select variant per pipeline, not assume one fixed load list.
2. Symbol-resolution step: code_ptr's address is discovered at run
   time by either (i) `cor24-run --run pvm.s -e code_ptr` parsing
   "Entry point: ... @ <addr>" (run-pascal.sh:26-27) or (ii)
   pre-assembling pvm.s and grepping pvm.lst (run-multi-unit.sh:88-90).
   Needs first-class "resolve symbol from artifact via method".
3. Scripted post-link relocator (relocate_p24.py) that walks p-code
   opcodes by length and rewrites data-segment refs. Schema must
   allow arbitrary post-link transforms with derived output names
   (`.p24` -> `.bin`).
4. Three pipeline variants (single-unit, single-file unit-mode,
   multi-file multi-unit) selecting different runtime artifacts:
   runtime/runtime.spc (pl24r) vs runtime/p24p_rt.p24 (p24-load).
5. Multi-unit input composition: compile once per unit -> .spc +
   .spi; then run main.pas with all .spi files prepended inside
   `;--- SPI <name> ---` ... `;--- END SPI ---` markers. Schema
   needs "compose UART input from N intermediate artifacts wrapped
   in literal markers".
6. UART output region extraction: success = trailer `; OK`; .spc
   delimited by `.module/.endmodule` or `.unit/.endunit`; .spi by
   `;--- SPI --- ... ;--- END SPI ---`.
7. Generated checked-in artifact: compiler/p24p.s is committed but
   regeneratable from compiler/src/*.c via tc24r. Schema should
   distinguish "checked-in derived artifact" from upstream sources.
8. EOT framing (`\x04`) is hand-rolled by all six scripts. Should
   be a schema default for `-u` UART inputs.
9. Sibling-relative tool paths at `$REPO_DIR/../sw-cor24-pcode/...`.
   No vendor/ pin file -- contrasts with plsw. Schema must model
   "sibling repo dep, relative path, no version lock".
10. No version pinning at all: cor24-run/tc24r/pa24r/pl24r/p24-load
    are taken from PATH or sibling target/release/. Schema needs an
    explicit "no pinning" stance surfaced in audits.
