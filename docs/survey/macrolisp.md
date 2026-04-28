# Survey: sw-cor24-macrolisp

A C-implemented Lisp-1 interpreter (built via tc24r to COR24 24-bit RISC
assembly and run on cor24-run) with five prelude tiers (bare, minimal,
standard, full, scheme), an interactive REPL fed via UART terminal, an
optional 10x faster startup path that restores interpreter heap state
from a binary "prelude.snap" snapshot loaded at 0x080000 via
--load-binary, a Lisp-to-asm compiler producing build/compiled.s, and a
multi-module demo that pre-assembles five service modules at fixed base
addresses (0x1000..0x5000) and side-loads them as raw binaries.

## 1. Run scripts

Top-level driver is `justfile`. Shell scripts under `scripts/` wrap it.

- `justfile` (root) -- canonical entry points: `build`, `test`,
  `run`, `run-fast`, `run-minimal`, `run-standard`, `run-scheme`,
  `run-full`, `run-bare`, `run-custom`, `eval`, `eval-scheme`,
  `eval-full`, `eval-custom`, `compile`, `run-compiled`,
  `run-compiled-uart`, `test-asm`, `snapshot`, plus `demo-blink`,
  `demo-bottles`, `demo-bottles2`, `demo-bottles4`, `demo-reduce`,
  `demo-continuations`, `demo-isr-echo`, `demo-multi` (justfile lines
  9-217).
- `scripts/build.sh` -- thin wrapper: checks `tc24r` and `cor24-run` are
  on PATH, then runs `just build && just test` (lines 7-26).
- `scripts/run-tests.sh` -- runs `just test`; with `-v` runs
  `cor24-run --run build/tml24c.s --speed 0 -n 10000000` and pipes
  `[UART TX` lines through `scripts/extract-uart.py` (lines 12-19).
- `scripts/repl.sh` -- `just build-repl` then
  `cor24-run --run build/repl.s --terminal --echo --speed 0` (line 14).
- `scripts/eval-expr.sh` -- pipes one expression into
  `cor24-run --run build/repl.s --terminal --speed 0 -n 200000000`
  (line 14).
- `scripts/load-eval.sh` -- builds repl, optionally prepends a prelude
  file, pipes through `cor24-run --run build/repl.s --terminal`
  (lines 41-53).
- `scripts/extract-snapshot.py` -- post-processes the hex UART dump
  produced by `snapshot-save.s`: scans for the `TML` magic, strips
  non-hex characters, hex-decodes, and writes
  `build/prelude.snap` with the 3-byte `TML` magic prepended
  (lines 6-22).
- `scripts/disasm.sh`, `scripts/profile.sh` -- dev helpers (not on the
  primary launch path).

## 2. Memory loads

`cor24-run` invocations carry the program as `--run <file.s>` and any
side-loaded binaries as `--load-binary <file>@<hex-addr>`.

Snapshot-accelerated REPL (`run-fast`, justfile line 53):

    cor24-run --run build/repl-snapshot.s \
              --load-binary build/prelude.snap@0x080000 \
              --terminal --echo --speed 0

The snapshot address `0x080000` is hard-coded as `SNAPSHOT_ADDR` in
`src/snapshot.h:3`. `repl-snapshot.c` checks that location for the
`TML` magic and, if present, restores all interpreter scalars and
arrays from there before entering the REPL (snapshot.h:57-106).

Multi-module demo (`demo-multi`, justfile lines 195-217) pre-assembles
five service modules with `cor24-run --assemble ... --base-addr <hex>`
to `build/<mod>.bin`/`build/<mod>.lst`, compiles `demos/multi/main.l24`
to `build/main.s`, then runs:

    cor24-run --run build/main.s \
              --load-binary build/uart.bin@0x1000  \
              --load-binary build/spi.bin@0x2000   \
              --load-binary build/i2c.bin@0x3000   \
              --load-binary build/gpio.bin@0x4000  \
              --load-binary build/timer.bin@0x5000 \
              --speed 0 -n 10000000

Other invocations carry no side-loads -- the main `.s` is everything.

## 3. Patches

No textual or binary patching of artifacts is performed. The repo never
modifies a built `.s` or `.bin` after the assembler runs. The only
post-processing is `scripts/extract-snapshot.py` extracting hex from a
UART log (not a patch -- it produces the snapshot blob, not a delta).

## 4. UART payload structure

UART is the universal I/O channel. Two payload styles in this repo:

1. Source text fed to a REPL/compiler. The `eval`, `run-custom`,
   `compile`, etc. recipes pipe `.l24` source (with `^;;` comment lines
   stripped via `grep -v '^;;'`) into stdin; `cor24-run --terminal`
   bridges stdin to UART RX. The compiler additionally appends a
   single `\004` (EOT) byte as end-of-input marker (justfile lines
   116-122, 128-134).
2. Snapshot save format. `snapshot-save.s` writes the full interpreter
   state to UART as ASCII hex (two chars per byte). The leading 3 bytes
   are the literal ASCII `TML`; the rest is a stream of 24-bit
   little-endian fields (`src/snapshot.h:12-49`):
   `heap_next, free_list, global_env, sym_count, name_pool_next,
   str_pool_next, gensym_counter`, then 11 pre-interned symbol IDs,
   then `heap_car[0..heap_next]`, `heap_cdr[0..heap_next]`,
   `name_pool[0..name_pool_next]`, `sym_name_off[0..sym_count]`,
   `str_pool[0..str_pool_next]`. `extract-snapshot.py` strips the hex
   and writes it back as binary, prepending the `TML` magic so the
   restore path at `SNAPSHOT_ADDR` recognises it
   (`src/snapshot.h:57-60`).

REPL output uses the same UART for prompts/results; `--echo` makes
cor24-run echo input bytes back to stdout.

## 5. Heaps and stacks

Documented in `docs/memory-usage.md` and `docs/architecture.md`.

- SRAM `0x000000-0x0FFFFF` (1 MB) holds code (~60 KB) and the static
  Lisp heap (`docs/memory-usage.md:5-37`).
- Lisp heap is three parallel C arrays in BSS: `heap_car[32768]`,
  `heap_cdr[32768]`, `heap_mark[32768]` (96 KB each, 288 KB total),
  plus `str_pool[8192]`, `name_pool[4096]`, `sym_name_off[512]`,
  `gc_roots[256]` (memory-usage.md:9-17). Sizing constants are
  `HEAP_SIZE` in `src/tml.h`, `STR_POOL_SIZE` in `src/string.h`,
  `MAX_SYMBOLS`/`NAME_POOL_SIZE` in `src/symbol.h`, `MAX_GC_ROOTS` in
  `src/gc.h` (memory-usage.md:188-196).
- Snapshot blob lands at `0x080000` (well above static data,
  well below the EBR stack region) -- `src/snapshot.h:3`.
- C call stack lives in EBR at `0xFEE000-0xFEFFFF` (8 KB), initial SP
  `0xFEEC00`. Conservative GC scans this region for tagged heap
  pointers (memory-usage.md:24-29, 54-64).
- I/O at `0xFF0000-0xFFFFFF`: `0xFF0000` LED/switch, `0xFF0010` IntEn,
  `0xFF0100` UART data, `0xFF0101` UART status (memory-usage.md:65-74).
- GC is mark-sweep; roots are `global_env`, `gc_roots[]`, and the
  conservative C-stack scan (memory-usage.md:148-155).

## 6. Build artifacts vs vendored

No `vendor/` directory exists. All shipped binaries are derived. The
repo has an `include/` directory but it is empty, and a `bin/tml24c`
host binary that is a build product (not vendored upstream code).

Build artifacts under `build/` (all generated by `just <recipe>`):

- `tml24c.s`, `repl-bare.s`, `repl-minimal.s`, `repl-standard.s`,
  `repl-full.s`, `repl-scheme.s`, `repl-snapshot.s`,
  `snapshot-save.s`, `compiler.s` -- one per `src/<name>.c` (justfile
  lines 11-46).
- `prelude.snap.raw` (UART hex log) and `prelude.snap` (binary blob,
  TML-prefixed) from `just snapshot` (justfile lines 49-54).
- `compiled.s`, `main.s`, `test-asm.s` -- products of running the Lisp
  compiler on `.l24` files.
- `uart.bin/.lst`, `spi.bin/.lst`, `i2c.bin/.lst`, `gpio.bin/.lst`,
  `timer.bin/.lst` -- pre-assembled service modules for `demo-multi`,
  built from `demos/multi/<mod>.s` via `cor24-run --assemble` with
  fixed `--base-addr` (justfile lines 199-203).
- `test-results.txt` -- harness output filtered from UART
  (justfile lines 178-182).

External tooling required on PATH: `tc24r` (C-to-COR24 compiler),
`cor24-run` (assembler+emulator), `just`, `python3`, `bash`, `grep`
(scripts/build.sh:9-16).

## 7. Known limits

- Snapshot is fragile: if a prelude definition changes, `prelude.snap`
  must be regenerated (`docs/prelude-loading.md:38-40`). The fallback
  path (no `--load-binary`) re-evaluates prelude via `eval_str`.
- Snapshot save passes interpreter state through UART as hex, capped by
  `cor24-run -n 50000000` (justfile line 53). Pure size limit lives in
  the cycle budget, not in the format.
- `SNAPSHOT_ADDR=0x080000` is hard-coded (`src/snapshot.h:3`); both
  generator and loader must agree.
- Multi-module demo addresses are hard-coded in both `main.l24`
  (`la r0,#x1000` etc.) and the justfile recipe -- no schema indirection.
- String pool and symbol names are append-only; GC does not reclaim
  them (memory-usage.md:166-172).
- C stack is only 3 KB by default (8 KB max with
  `--stack-kilobytes 8`), and GC scans it conservatively
  (memory-usage.md:54-64).
- Tests rely on UART-line grep matching exactly five `^(scaffold|reader
  |eval|gc|compile) ok$` lines; any extra noise fails the suite
  (justfile lines 169-177).

## Schema gaps for this repo

Any sw-launcher schema must express:

- A program artifact (`--run <file.s>`) plus N optional side-loaded
  binaries each carrying a hex base address (`--load-binary FILE@ADDR`).
- A pre-build pipeline that can run other tools first
  (`cor24-run --assemble SRC OUT.bin OUT.lst --base-addr ADDR`) and
  feed their outputs into the side-load list.
- Multiple alternative entry binaries for the same project (five REPL
  variants plus snapshot, compiler, snapshot-save) selected by recipe
  -- not one canonical `target/`.
- Stdin source piped to UART, optionally with a content filter
  (`grep -v '^;;'`) and an optional EOF sentinel byte (`\004`).
- Cycle budget `-n N`, terminal mode flag, echo flag, and clock speed
  (`--speed 0` vs `--speed 500000` for the blink demo).
- A "snapshot generation" recipe whose output flows into a later
  `--load-binary` invocation, with a host-side post-processor
  (`scripts/extract-snapshot.py`) sitting between two emulator runs.
- Output filters to strip emulator boilerplate
  (`Assembled `, `Executed N instructions`, `^[CPU`).
- Memory-map metadata (snapshot address, I/O base, stack base, heap
  array sizing) so a launcher can sanity-check `--load-binary` targets
  do not overlap heap or stack.
- Per-recipe environment expectations (PATH must contain `tc24r` and
  `cor24-run`; no other vendored toolchain).
