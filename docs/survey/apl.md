# Survey: sw-cor24-apl

A minimal APL interpreter for the COR24 24-bit RISC ISA, written in C
and compiled with tc24r to a single COR24 assembly artifact
(build/apl.s). It runs on the cor24-run emulator either as an
interactive UART REPL or in batch mode where an APL "image" (plain
text, newline-separated APL lines) is loaded into SRAM at 0x080000
and an image-pointer cell at 0x09FF00 is patched to point at it. The
interpreter detects the image pointer at startup and reads program
lines from SRAM instead of UART.

## 1. Run scripts

- /Users/mike/github/sw-embed/sw-cor24-apl/build.sh -- top-level
  driver. Compiles src/main.c with tc24r to build/apl.s, then runs
  cor24-run. Two modes:
  - Interactive (no --batch): emits roughly
    `cor24-run --run "$BUILD_DIR/apl.s" "$@"` (build.sh:77).
  - Batch (--batch <file.a24>): emits
    `cor24-run --run "$BUILD_DIR/apl.s" --load-binary "$batch_file@0x080000" --patch "0x09FF00=0x080000" "$@"`
    (build.sh:72-75; APL_IMAGE_PTR/APL_IMAGE_BASE at build.sh:38-39).
- /Users/mike/github/sw-embed/sw-cor24-apl/test.sh -- iterates
  samples/batch-*.a24 and horse-race*.a24, calling
  `./build.sh run --batch "$a24" --speed "$SPEED"` with SPEED default
  5000000 (test.sh:10, 27, 32, 65). Compares scrubbed UART output
  against samples/<name>.expected.
- /Users/mike/github/sw-embed/sw-cor24-apl/scripts/build.sh -- thin
  wrapper that exec's the top-level build.sh (scripts/build.sh:10).
- /Users/mike/github/sw-embed/sw-cor24-apl/samples/test-cor24.sh --
  invokes cor24-run directly. Two distinct invocations:
  - Interactive .cor24 samples (line 39):
    `cor24-run --run "$PROJECT/build/apl.s" --stack-kilobytes 8 -n 20000000 $emu_flags -u "$uart_input"`
    where $emu_flags is `--switch on` for *-pressed* variants
    (line 36) and $uart_input is the .cor24 file with each line
    suffixed by literal "\n" (lines 30-32).
  - Batch .a24 samples (line 88):
    `cor24-run --run "$PROJECT/build/apl.s" --stack-kilobytes 8 -s 0 -n 50000000 --load-binary "$apl@0x080000" --patch "0x09FF00=0x080000"`.
- /Users/mike/github/sw-embed/sw-cor24-apl/samples/test-gnu-apl.sh --
  reference-implementation harness; runs samples/*.apl through GNU
  APL (`apl --script --noCIN --noSV < "$apl"`), no COR24 involvement
  (test-gnu-apl.sh:18). Listed for completeness only.

## 2. Memory loads

| layer-name | path | address | source | rationale |
|------------|------|---------|--------|-----------|
| apl_interp | build/apl.s | (loaded by --run, default text base) | built locally via `tc24r src/main.c -o build/apl.s -I /Users/mike/github/sw-embed/sw-cor24-x-tinyc/include` (build.sh:32) | the interpreter ELF/asm itself; cor24-run --run handles its placement |
| apl_image (batch only) | $batch_file (e.g. samples/batch-*.a24) | 0x080000 | user-authored APL text image | data fed to the interpreter; placed in upper SRAM well above heap (heap is in interpreter .bss in low SRAM) per docs/architecture.md:208-211 and docs/batch-mode.md:32-44 |

Interactive mode emits no `--load-binary` (only `--run apl.s`).

## 3. Patches

| target | value | source-of-symbol | rationale |
|--------|-------|------------------|-----------|
| 0x09FF00 | 0x080000 | C macro `APL_IMAGE_PTR` (src/main.c:18) and shell var `APL_IMAGE_PTR` (build.sh:38); APL_IMAGE_BASE matches in src/main.c:19 / build.sh:39 | sets the "image present" flag word; interpreter reads this at startup and, if non-zero, switches to batch mode and reads APL source lines from the address it points to (docs/batch-mode.md:38-44) |

Only one patch site exists, and only in batch mode. Interactive mode
emits no --patch.

## 4. UART payload structure

Interactive REPL samples (samples/*.cor24) are streamed via -u as a
single string. Construction is in samples/test-cor24.sh:30-33:

```
while IFS= read -r line || [ -n "$line" ]; do
    uart_input="${uart_input}${line}\\n"
done < "$cor24"
```

Each .cor24 line becomes one APL command, terminated by literal
backslash-n (cor24-run interprets "\n" as 0x0A line feed). There is
no EOT/0x03/0x04 trailer; input simply ends. The interpreter loops on
its UART REPL until either ")OFF" appears in the input (executed as
a system command -- see README.md:60) or the cycle budget `-n 20000000`
expires.

For batch mode (samples/test-cor24.sh:88-89, build.sh:72-75) UART is
not pre-populated -- the program text comes from the SRAM image, and
UART is left free for any quad-SVO output the program emits.

## 5. Heaps and stacks

- name: heap (the APL value heap)
  - embedded in artifact: yes -- C global `int heap[HEAP_SIZE]` in
    src/arr.h:14,22 (HEAP_SIZE = 4096 ints). Lives in build/apl.s
    .bss as part of the compiled interpreter; no loader reservation.
  - grows up: yes (bump allocator; `heap_top` increments, see
    src/arr.h:23 and the `heap_top = heap_save` reclamation pattern
    in src/main.c:357-419).
  - runtime symbols: `heap`, `heap_top`, `arr_reset()` (src/arr.h:22-28).
  - address chosen: implicit -- placed by the linker in the
    interpreter's .bss. docs/architecture.md:204 estimates "~0x012000
    Heap (arrays) grows upward" but the C source does not pin an
    address. The 4096-int (12 KB at 3 bytes/word) cap is the practical
    limit, not an SRAM region.

- name: prog_buf (program line buffer for stored multi-line programs)
  - embedded in artifact: `char prog_buf[7680]` (src/main.c:47), 64
    lines x 120 chars (src/main.c:44-45). C global; lives in .bss.
  - grows up: indexed addressing, no growth direction.
  - runtime symbols: `prog_buf`, `prog_count`, `prog_store/get/clear`
    (src/main.c:47-73).

- name: COR24 EBR stack (CPU return/data stack)
  - reserved by the loader / emulator: `--stack-kilobytes 8` is set
    in samples/test-cor24.sh:39 and :88. Not set by build.sh (relies
    on cor24-run default).
  - grows down: per docs/architecture.md:216-218, "Stack (EBR) ...
    grows downward" at 0xFEE000-0xFEFFFF.
  - runtime symbol: sp register; no source-level symbol exported.
  - address chosen: fixed by the COR24 platform spec, not by this
    repo (docs/architecture.md:43, CLAUDE.md:87).

The repo declares no eval_stack / call_stack / dedicated value-heap
segment in source (no labelled `eval_stack:` or `heap_seg:` in any
.s file -- asm/ contains only a .gitkeep). All "heaps" here are C
globals living in .bss of the single compiled artifact.

## 6. Build artifacts vs vendored

Built locally:
- build/apl.s -- only artifact, produced by
  `tc24r src/main.c -o build/apl.s -I /Users/mike/github/sw-embed/sw-cor24-x-tinyc/include`
  (build.sh:32). Includes pulled from a sibling tc24r checkout.

Consumed from PATH (not vendored in-tree, no vendor/ directory at
all -- confirmed: `ls` shows no vendor/, no active.env, no version.json):
- tc24r -- C compiler (build.sh:18; CLAUDE.md:28-30 names the
  source repo `sw-cor24-tinyc`, with includes coming from
  `sw-cor24-x-tinyc/include`).
- cor24-run -- emulator/assembler combined (build.sh:19;
  CLAUDE.md:30 names source repo `sw-cor24-emulator`).

No version pins are recorded anywhere in the repo. README.md:25 only
says "Requires `tc24r` (C compiler) and `cor24-run` (emulator) on
PATH." Whatever is on PATH at build time is what gets used.

## 7. Known limits

- HEAP_SIZE = 4096 ints (src/arr.h:14). Exhaustion -> "WS FULL" error
  per README.md:64-65.
- Program buffer: PROG_MAX = 64 lines, PROG_LINE = 120 chars,
  prog_buf = 7680 bytes (src/main.c:44-47).
- Token buffer: 128 tokens per line (docs/design.md:119-120).
- Symbol table: 64 entries (docs/design.md:178-179).
- Maximum APL rank: 2 (matrix); design decision D5 in docs/design.md:65-72.
- Cycle budgets enforced by the test harness, not the interpreter:
  - test.sh: SPEED = 5000000 cycles/sec via --speed (test.sh:10).
  - samples/test-cor24.sh: -n 20000000 for interactive REPL tests
    (line 39); -s 0 -n 50000000 for batch tests (line 88).
- Image size (batch mode): bounded only by SRAM headroom above
  0x080000 -- "roughly 512 KB with the default base at 0x080000"
  per docs/batch-mode.md:43-44. No script-enforced cap.
- Max source line: 256 bytes (docs/design.md:284 -- line buffer).

## Schema gaps for this repo

- need a layer.kind value like "text-image" (or
  layer.format = "text") to express that samples/batch-*.a24 is plain
  newline-separated APL source, not a compiled binary -- the launcher
  schema today assumes binary blobs at fixed offsets.
- need a per-scenario "text image" terminator policy (this repo
  relies on emulator zero-fill from --load-binary so no explicit
  trailer is written, but ")OFF" is the in-band terminator); a field
  like `layer.text_terminator = "trailing-null" | "EOT" | "EOF"`
  would let the launcher describe it.
- need a way to express "image-present pointer" patches symbolically:
  patches today look like `0x09FF00=0x080000` -- the launcher should
  permit `patch.target = "<layer>.image_ptr"` and
  `patch.value = "<layer>.base"` to avoid hardcoded hex.
- need scenario-level toggle between interactive (uart_input fed,
  cycle budget -n 20000000) and batch (image loaded, -s 0 unlimited
  speed, -n 50000000) -- two distinct cor24-run invocation shapes
  per scenario family.
- need per-invocation overrides for `--stack-kilobytes`,
  `--speed`/`-s`, and `-n` cycle budget; today they are scattered
  across scripts (8 KB, 5e6, 2e7, 5e7) with no single place to
  declare them.
- need a `uart_input.encoding = "literal-backslash-n"` field to
  capture how samples/test-cor24.sh:31 builds the payload (literal
  "\\n" between lines, no real 0x0A).
- need an optional `emulator_flags` map keyed by sample-name pattern
  (e.g. "*-pressed* -> --switch on" from
  samples/test-cor24.sh:36) -- the schema currently has no place
  for variant flags driven by filename suffix.
- need a "tool from PATH, no version pin" source kind for tc24r and
  cor24-run, since this repo has no vendor/ directory and no
  version.json -- the schema should not force a pin where the repo
  declares none.
- no need for an explicit eval_stack / heap_seg segment declaration
  for this repo: all dynamic memory is C-global .bss inside the
  single built artifact. The schema should permit a layer to declare
  zero segments and rely on linker-allocated bss.
