# Survey: sw-cor24-snobol4

A SNOBOL4-inspired bytecode interpreter written in PL/SW, targeting the
COR24 24-bit emulator. The interpreter is built from four PL/SW modules
(`sno_main`, `sno_util`, `sno_lex`, `sno_exec`) compiled to assembly,
linked through a multi-pass pipeline that uses `meta-gen` (extern
placeholders + symbol export tables) and `link24` (concatenation +
FIXUP patching) into a single image `build/snobol4.bin` loaded at
COR24 address 0. The SNOBOL source program is loaded as a separate
`--load-binary` blob at `0x080000`, optional batch input data at
`0x090000`; absence of data at `0x090000` flips the interpreter into
live-UART (TTY) input mode.

## 1. Run scripts

Three driver scripts under `scripts/`, all funneling through
`build-modular.sh` and `cor24-run`:

- `scripts/run-snobol4.sh` -- batch mode. Loads interpreter@0,
  source@0x080000, optional data@0x090000, runs with
  `-n 200000000 -t 120 --speed 0 --dump`, then awks UART output
  out of the dump (`scripts/run-snobol4.sh:66-79`).
- `scripts/run-snobol4-tty.sh` -- interactive mode. Same first two
  loads but **deliberately omits** the data load at `0x090000` and
  runs `cor24-run --terminal` with `-n -1 -t -1`. The script's own
  comment at `scripts/run-snobol4-tty.sh:39-45` flags the omission as
  load-bearing: the interpreter probes that address and switches
  `READ_INPUT` to UART when it reads a null byte.
- `scripts/demo-hello.sh` -- canned end-to-end demo for `examples/hello.sno`.
  Builds, loads at 0 and 0x080000, runs with `--dump`, redirects to
  `build/hello.dump.txt`, awks UART output back out
  (`scripts/demo-hello.sh:31-44`).

`justfile` recipes (`build`, `rebuild`, `run`, plus per-example shortcuts
like `hello`, `count`, `span`, `input`, `array`) all delegate to those
shell scripts (`justfile:4-62`).

## 2. Memory loads

`run-snobol4.sh` (and `demo-hello.sh`) issue this exact load set
(`scripts/run-snobol4.sh:66-75`):

```
cor24-run --load-binary build/snobol4.bin@0
          --load-binary <prog.sno>@0x080000
          [--load-binary <data.dat>@0x090000]
          --entry 0
```

- Interpreter image: a single `snobol4.bin` placed at `0x000000`.
  Internally it is the link24 concatenation of four modules (see
  section 6); the linker assigns each module a contiguous base in the
  same image, so callers see one flat blob, not four `--load-binary`
  arguments.
- SNOBOL source: raw text at `0x080000` (`SRC_LOAD_ADDR=524288`,
  `include/snoglob.msw:64`). `READ_SRC` byte-copies up to
  `SRC_LIMIT=12280` bytes into the in-image `SRC` buffer
  (`src/sno_util.plsw:32-55`, `include/snoglob.msw:103-104`).
- Data file (batch): raw text at `0x090000`
  (`INP_LOAD_ADDR=589824`, `include/snoglob.msw:65`). `READ_INPUT`
  scans byte-by-byte from there (`src/sno_util.plsw:104-123`).
- Entry: `--entry 0`. The image's first instruction is `_start`
  (assembler convention), which falls through to `_MAIN`
  (`build/mod/snobol4.map:3,12`).

## 3. Patches

There is **no host-side patch step** at runtime. All address fix-ups
happen at build time inside `link24`:

- `meta-gen prep` walks each `.s` and rewrites externally-resolved
  `la rN, _SYM` instructions to `la rN, 0` placeholders, recording the
  symbol table in `<mod>.syms` (`scripts/build-modular.sh:91-97`).
- After pass-2 assembly with each module's chosen base, `meta-gen emit`
  produces `<mod>.meta` containing `EXPORT <sym> <off>` and
  `FIXUP <off> <sym>` lines (e.g. `build/mod/sno_main.meta`'s
  `FIXUP 0x006E _READ_SRC` against `EXPORT _READ_SRC 0x0075` from
  `sno_util.meta`).
- `link24` concatenates module binaries and patches each FIXUP word
  in place using global addresses, emits `build/snobol4.bin` and
  `build/mod/snobol4.map` (`scripts/build-modular.sh:136-140`).

The runtime image is therefore self-contained; the launcher just
loads it at 0 and jumps.

## 4. UART payload structure

Output is plain ASCII bytes streamed to the COR24 UART TX port via
the `_UART_PUTCHAR` / `_UART_PUTS` routines exported by `sno_main`
(`build/mod/snobol4.map:5-11`). No framing or header -- the SNOBOL
program prints whatever it prints.

Capture path:

- Batch runs use `cor24-run ... --dump`. The dump contains a
  `UART output:` block which `run-snobol4.sh:79` and
  `demo-hello.sh:40` extract with awk, stopping at the next
  `Executed ...` stat line.
- TTY mode (`run-snobol4-tty.sh:41`) uses `--terminal` so UART
  TX/RX are wired to the host stdio directly; no awk extraction.

Input (TTY mode) is a single line at a time read via
`_UART_GETCHAR`, with local echo, backspace, and Ctrl-D as EOF
implemented in PL/SW (`src/sno_util.plsw:67-101`). Auto-uppercase is
applied on read.

## 5. Heaps and stacks

The interpreter has **no separate runtime heap arena** loaded at a
host-chosen address; all dynamic state lives inside statically-sized
buffers compiled into `sno_main`'s data section
(`include/snoglob.msw:97-228`):

- `SRC` -- 12 KiB source buffer (`SRC_SIZE=12288`).
- `SB` -- 64 KiB string buffer (`SB_SIZE=65536`) with soft-limit
  compaction at 60 KiB (`SB_SOFT_LIMIT=61440`) and a forwarding-table
  GC mechanism (`SB_FWD_*`, `SB_FWD_MAX=128`).
- `SYM*` / `LBL*` -- symbol and label tables sized by `SYMMAX=64`,
  `LBL_MAX=64`.
- Statement table (`S_*`, `EP_*`, `PP_*`) sized by
  `STMAX=256`, `EPSLOTS=8`, `EPMAX=2048`.
- `AM_CODE` -- 4 KiB AM (abstract machine) bytecode buffer.
- `ESTK`/`ETYP` -- 256-deep evaluation stack.
- `VARS(SYMMAX)` -- variable slot per symbol.
- `ARR_DATA`/`ARR_TYP` -- array pool of `ARR_MAX=8` arrays of
  `ARR_ELEMS=50` (`ARR_POOL=400`).
- `PSTK`/`PSTYP` -- 16-deep pattern stack.
- `CSTK_*` -- 16-deep call stack for user functions.

Concrete addresses (post-link, see `build/mod/snobol4.map`): `_SRC`
at 0x0000FA, `_SB` at 0x003192, `_SYMV`/`_VARS`/`_ESTK` etc. all
within the `sno_main` module range (0x000000-0x01EBD2). The semantic
stacks (eval, pattern, call) are **heap-backed arrays in this BSS,
not the COR24 hardware stack** -- consistent with
`docs/architecture.md:43-77,179-194`.

The COR24 machine stack itself is set up by `_start` per PL/SW
convention; the launcher does not configure it.

## 6. Build artifacts vs vendored

No `vendor/` directory exists. Everything is built from sources in
this repo plus two host-side toolchain binaries:

- Sources: `src/sno_main.plsw`, `src/sno_util.plsw`, `src/sno_lex.plsw`,
  `src/sno_exec.plsw` plus shared headers under `include/`
  (`descr.msw`, `heap.msw`, `am.msw`, `pat.msw`, `snoglob.msw`).
- External tools (paths hard-coded in `scripts/build-modular.sh:30-32`):
  - `cor24-run` (assembler + emulator) -- expected on `$PATH`.
  - `link24` and `meta-gen` -- expected at
    `$HOME/github/sw-embed/sw-cor24-plsw/components/linker/target/release`.

Pipeline (`scripts/build-modular.sh:36-154`), entry module first then
libraries (`ENTRY=sno_main`, `LIBS=sno_util sno_lex sno_exec`):

1. Per module: PL/SW compile to `build/<mod>.s`.
2. `meta-gen prep` rewrites externs to placeholders ->
   `build/mod/<mod>_prep.s`, with `<mod>.syms`.
3. Pass-1 assemble at base 0 to measure module size in bytes.
4. Compute contiguous bases: sno_main@0x00000, sno_util@0x1EBD3,
   sno_lex@0x1FE0E, sno_exec@0x234E5 (observed sizes 125907 / 4667 /
   14041 / 19583 bytes; total 164198).
5. Pass-2 assemble each module with `--base-addr <base>`.
6. `meta-gen emit` writes `<mod>.meta` (EXPORT + FIXUP entries) from
   the pass-1 listing + syms.
7. `link24 --entry sno_main --dir build/mod sno_main sno_util sno_lex sno_exec
   -o build/snobol4.bin` concatenates and applies FIXUPs.
8. `build/.build-deps` records mtimes for staleness skip.

`build/sno_*.s` and `build/sno_*-dump.txt` are intermediate single-module
artifacts (the `-dump.txt` files for stand-alone modules show
`Undefined label '_READ_SRC'` etc., confirming each module is incomplete
on its own and depends on the link step).

## 7. Known limits

- Source size is hard-capped at `SRC_LIMIT=12280` bytes; overflow
  sets `SB_OVERFLOW` and emits "SNOBOL4: source too large, truncated
  at byte=" (`include/snoglob.msw:215`, `src/sno_util.plsw:46-54`).
- String buffer is 64 KiB with soft compaction; hard abort at
  `SB_LIMIT=65024` (`include/snoglob.msw:120-122`).
- 64 symbols, 64 labels, 256 statements, 8x50 array pool,
  16-deep pattern + call stacks, 4-function user-defined-function
  table (`include/snoglob.msw:68-198`).
- Batch-mode TTY-vs-data discrimination is implicit: if the byte at
  `0x090000` is zero, `_MAIN` flips `INP_TTY=1`
  (`src/sno_main.plsw:14-17`). Loading **anything** at `0x090000`
  forces batch mode -- `run-snobol4-tty.sh:39` calls this out.
- Module split is locked: `CLAUDE.md:5-32` forbids monolithic
  `snobol4.plsw`/`sno_engine.plsw`/`snolib.plsw` revivals.
- Toolchain paths in `scripts/build-modular.sh:30-32` are hard-coded
  to `$HOME/github/sw-embed/sw-cor24-plsw/...`.

## Schema gaps for this repo

A sw-launcher schema must capture, beyond a single
`{binary, entry, source@addr, data@addr}` shape:

1. **Composite binary built from N modules** with their own contiguous
   bases. The user-visible artifact is one image at 0, but the survey
   record (and any incremental rebuild) needs the per-module list and
   ordering (entry first), since `link24`/`meta-gen` care.
2. **Optional payload that doubles as a mode selector.** The presence
   or absence of a load at `0x090000` is itself a runtime signal
   (`INP_TTY` probe). The schema needs "load only if file given" plus
   a per-load "presence-implies-mode" annotation or the launcher
   cannot reproduce `run-snobol4-tty.sh` correctly.
3. **Two run profiles per program**: dump-and-grep batch mode
   (`--dump`, awk-extracted UART) vs. interactive `--terminal` mode
   (`-n -1 -t -1`). Same image, different `cor24-run` flag set,
   different output capture strategy.
4. **External tool dependencies with hard-coded host paths**
   (`link24`, `meta-gen` under a sibling repo). The schema should
   model toolchain locations rather than embedding `$HOME/github/...`.
5. **Per-image static buffer layout** (SRC, SB, SYM, ESTK, etc.) is
   inside the image, not separately loaded. The schema should
   distinguish "host loads at addr" from "interpreter-internal BSS"
   so that surveys for repos which DO use a separate heap arena (e.g.
   the `0x080000-0x0FFFFF` arena suggested in
   `docs/running-plsw-compiler.md:174` for an alternate design)
   can be expressed without conflating them.
6. **Source-size limits and overflow semantics** are policy of the
   loaded image (`SRC_LIMIT`), but the launcher needs to know them so
   it can pre-validate or warn -- a "max payload bytes per slot"
   field per load address.
