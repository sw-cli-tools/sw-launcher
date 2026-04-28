# Survey: sw-cor24-script

This repo provides "sws" (Software Wrighter Script), a Tcl-like
command-language interpreter that runs natively on COR24. It is a
single-binary REPL: the emulator preloads sws.bin at address 0,
sws prints "sws 0.1", prints the prompt "sws> ", and reads one
line at a time from UART. It cannot load or assemble source code
itself -- there is no in-process assembler or compiler. To launch
another program (e.g. the swye editor), sws expects the other
binary to be co-loaded by the emulator at a known address, with
its entry-point word patched into a fixed table at 0x0FFE00; the
"run" command reads that address and calls it as a function
pointer, then reads back results from a shared output buffer at
0x0F0400. All other "load/edit" operations (the filesystem
commands, source) are stubs that depend on a COR24 OS that does
not yet exist.

## 1. Run scripts / entry points

`scripts/build.sh` is the canonical build:
- `./scripts/build.sh` -- runs tc24r on `src/sws.c` to produce
  `build/sws.s`, then `cor24-run --assemble` to produce
  `build/sws.bin` and `build/sws.lst` (build.sh lines 22-29).
- `./scripts/build.sh run` -- builds and `cor24-run --run` on the
  assembly (build.sh lines 31-36).
- `./scripts/test.sh` -- 100+ unit tests that pipe scripts to
  `cor24-run --load-binary "$BIN@0" --entry 0 -u "<script>"`
  (test.sh lines 21, 37 onward).

The README's "Bootstrapping" pipeline (README.md lines 213-218):
tc24r compiles `sws.c -> sws.s`; `cor24-run --assemble` (or the
external `as24`) produces `sws.bin`; `sws.bin` runs on COR24 FPGA.
The toplevel `sws.bin`, `sws.lst`, `sws.s` and the parallel
`build/sws.bin`, `build/sws.lst`, `build/sws.s` are checked-in
copies of that pipeline output.

The richer entry point is `docs/examples/editor-demo.sh`, which
co-loads four binaries and shows the run-by-fnptr handoff to
swye.

## 2. Memory layout

From `src/sws.c` lines 12-20 (well-known addresses) and the
listing addresses in `sws.lst`:

| Region              | Address    | Notes                              |
|---------------------|------------|------------------------------------|
| sws code start      | 0x000000   | `_start`, listing line 1           |
| sws `_main`         | 0x00504C   | listing line 12145                 |
| `_user_halt`        | 0x0020CB   | `bra _user_halt` (sws.lst:5252)    |
| sws data (token_pool)| 0x00514C  | listing line 12285                 |
| `_line_buf`         | ~0x00514C+ | UART input buffer (512 B)          |
| `_eval_line_buf`    | ~          | re-eval scratch (512 B)            |
| `_source_buf`       | ~0x00C4AC  | source script buffer (4096 B)      |
| `_source_line_buf`  | ~0x00D4AA  | per-line source buffer             |
| `_penv_name_pool`   | ~0x00D6AA  | env name pool (1024 B)             |
| `_penv_val_pool`    | ~0x00DAAA  | env value pool (4096 B)            |
| String literals     | up to 0x00F3D2 | end of `.byte` data            |
| Run command buffer  | 0x0F0000   | `RUN_CMD_BUF` (1024 B, sws.c:16)   |
| Run output buffer   | 0x0F0400   | `RUN_OUT_BUF` (4096 B, sws.c:18)   |
| Run entry slot 0    | 0x0FFE00   | `RUN_ENTRY_0` (sws.c:20)           |
| UART data (MMIO)    | 0xFF0100   | sws.c:12                           |
| UART status (MMIO)  | 0xFF0101   | sws.c:13                           |

The interpreter image fits in roughly the first ~0xF400 bytes
(code + static data). The 0x0F0000-0x0FFFFF range is reserved for
the "run another binary" protocol: command buffer, output buffer,
and entry table. Anything at or above 0x100000 is unused by sws
itself and is effectively program-slot space for whatever binary
sws calls.

Static allocation limits (sws.c:70-77, language-reference.md
411-423):
| Resource          | Limit     |
|-------------------|-----------|
| Line length       | 512       |
| Tokens per line   | 32        |
| Token length      | 128       |
| token_pool        | 2048 B    |
| Variables         | 64        |
| Values            | 64        |
| Commands          | 48        |
| Source nesting    | 16        |
| Source buffer     | 4096 B    |
| Env vars          | 32        |
| Env name pool     | 1024 B    |
| Env value pool    | 4096 B    |
| Cmd-sub nesting   | 64        |
| While iterations  | 10000     |

## 3. Transfer of control

Two transfer-of-control paths:

(a) sws -> user program via `run`. `cmd_run` (sws.c lines 1638-
1690) is the whole story:
1. Resolve program name to entry address. `run_resolve` (sws.c:
   1631-1636) is currently hardcoded: only `swye` is recognized,
   and its entry is read as a 24-bit word from `RUN_ENTRY_0`
   (0x0FFE00) using `mem_read_word` (sws.c:1623-1629). Anything
   else returns 0 = "not loaded."
2. If args were given, copy them into `RUN_CMD_BUF` at 0x0F0000
   (sws.c:1661-1670). If no args, the buffer is left untouched
   so the emulator's preloaded keystrokes survive.
3. Zero `RUN_OUT_BUF[0]` to clear any prior output (sws.c:1672).
4. `int (*prog_main)() = entry; int result = prog_main();`
   (sws.c:1674-1675) -- a direct C function-pointer call. There
   is no trampoline, no stack switch, no SP save. The user
   program returns to sws by a normal function return.
5. On return, if `pragma run-rc on` is set, sws builds a record
   `{run, kind, prog, output}` from the result code and the
   contents of `RUN_OUT_BUF` (sws.c:1677-1687) and assigns it to
   the script variable `rc`.

(b) Halt at exit. `cmd_exit` (sws.c:920-927) just sets
`exit_flag`; `main`'s loop (sws.c:2326-2335) exits and calls
`halt()` (sws.c:960-963), which is two inline-asm instructions:
`_user_halt: bra _user_halt` -- a tight loop, not a return.

The entry-point table at 0x0FFE00 is populated by the emulator
via `--patch`. The editor demo (`docs/examples/editor-demo.sh`
lines 35, 56) finds swye's `_main` from its assembled listing
and passes `--patch "0x0FFE00=0x0${SWYE_MAIN}"`. There is no
runtime registration call -- registration is just a memory
patch.

## 4. UART / command set

Single REPL surface. `main` (sws.c:2307-2337) prints the banner,
then loops: print "sws> ", call `read_line` (sws.c:933-956) which
collects characters until newline (treating Ctrl-D = 0x04 as
EOF), then `eval_line(line_buf)`. read_line tracks brace depth so
multi-line `{ ... }` blocks can be entered interactively.

Builtins are registered by `register_builtins` (sws.c:2253-2303)
via a function-pointer dispatch table (`cmd_handler[MAX_CMDS]`,
sws.c:742, 752). The full command set:

I/O and variables: `echo`, `set`, `exists?`, `incr`.

Comparison: `eq`, `ne`, `lt`, `gt`, `le`, `ge`. Logic: `and`,
`or`, `not`. Arithmetic: `+`, `-`, `*`, `/`, `%`.

Strings: `concat`. Control flow: `if`, `while`, `break`,
`continue`. Termination: `exit`.

Process: `pragma run-rc on` (sws.c:2283), `run` (sws.c:2284) --
the only way to transfer control to another binary.

Filesystem (all stubs except `cd`/`pwd`): `cd`, `pwd`, `ls`,
`mkdir`, `rm`, `mv`, `cp`, `stat`, `fexists`. Source/env:
`source`, `env`. Debug: `_valtest`, `_toktest`.

Notably absent: no `load`, no `dump`, no memory inspect, no
`asm`/`compile`. There is no command for loading a `.bin` from
UART. The `source` command (sws.c:1950-2006) reads a file via
`fs_read_file`, but `fs_read_file` is itself a stub returning
-1 (see test.sh line 251 expecting `error: source: cannot read:`).

## 5. Heaps and stacks

No heap. tc24r forbids `malloc`/`free`; everything is statically
allocated. The major static buffers (with addresses from `sws.lst`
or sizes from `sws.c`):

- `token_pool[2048]` at 0x00514C+ (sws.c:170, listing 12285)
- `line_buf[512]` (sws.c:931)
- `eval_line_buf[512]` (sws.c:1130)
- `source_buf[4096]`, `source_line_buf[512]` (sws.c:1945-1946)
- `penv_name_pool[1024]`, `penv_val_pool[4096]` (sws.c:2014-2015)
- Value pool, environment array, command table (parallel arrays
  bounded by `MAX_VALS=64`, `MAX_VARS=64`, `MAX_CMDS=48`)
- `block_stack_pool` and `block_stack_top` (referenced from
  sws.lst:6028+) for nested block evaluation, replacing recursion

A mark-compact GC runs on the value pool at each `while`
iteration to reclaim values (CHANGES.md lines 12-14) -- without
it, MAX_VALS=64 would exhaust after ~10 iterations.

Single shared C stack. `_start` (sws.lst lines 1-3) loads `_main`
and jumps with `jal r1,(r0)`; SP is whatever the COR24 reset
state provides. When `cmd_run` calls a user program by function
pointer, the same C stack is reused -- there is no SP swap. The
user program is expected to be a well-behaved C function: take
no arguments, return an int, restore SP. The editor demo passes
`--stack-kilobytes 8` (editor-demo.sh line 57) to grow stack
beyond the default for the combined sws + swye footprint.

## 6. Build artifacts vs vendored

Generated, not vendored:
- `build/sws.s`, `build/sws.bin`, `build/sws.lst` -- output of
  `./scripts/build.sh`
- `build/test_fnptr.s` -- a one-off function-pointer experiment
- top-level `sws.s`, `sws.bin`, `sws.lst` -- a checked-in mirror
  of build output (timestamps suggest these are intentionally
  committed alongside source so the repo is runnable without a
  prior tc24r build)

True inputs:
- `src/sws.c` -- single 2337-line C source
- `scripts/build.sh`, `scripts/test.sh`, `scripts/test-all.sh`
- `docs/examples/*.sws` -- example scripts
- `docs/examples/editor-demo.sh` -- multi-binary demo

External tooling required: `tc24r` (cross C compiler at
`/Users/mike/github/sw-embed/sw-cor24-x-tinyc`, build.sh:16) and
`cor24-run` (emulator with `--assemble`, `--load-binary`,
`--patch`, `--run` modes). Nothing is precompiled and shipped
binary-only -- the bin files are reproducible from sws.c.

## 7. Edit-and-run model

(a) Edit source inside sws: NO. sws is a script-language REPL,
not an editor. There is no buffer/cursor concept; lines are read
one at a time and evaluated immediately (sws.c:2326-2333). To
edit a source script, you would invoke the swye editor as a
co-loaded binary via `run swye` (README.md lines 49-94).

(b) Load source from UART and assemble/compile in place: NO.
There is no assembler or compiler in sws. The only "load source"
mechanism is `source <file>` (sws.c:1950-2006), which calls
`fs_read_file` -- a stub. Even when working, `source` reads .sws
script text, not assembly or C source, and routes it through the
same `eval_line` interpreter. There is no path from textual COR24
assembly or C inside sws to executable code.

(c) Load pre-assembled .bin files: YES, but loading is performed
by the emulator before sws starts (`--load-binary file@addr`).
sws's role is limited to discovering the entry address from the
patched table at 0x0FFE00 and calling it (`cmd_run`,
sws.c:1638-1690). There is no UART .bin upload command, no
S-record/Intel-hex parser, no in-process loader.

The "edit a buffer" capability exists at the stack level: the
demo loads keystrokes at 0xF0000 and a text file at 0x10000,
runs swye via `run swye`, and sws then reads the edited buffer
back through `$rc.output` (which sws populates from
`RUN_OUT_BUF` at 0x0F0400). But sws itself is just the
orchestrator -- the editing happens in swye, and source code
never gets re-assembled.

Caching of compiled artifacts: NO. Every `./scripts/build.sh`
invocation re-runs tc24r and the assembler. The committed
top-level `sws.bin`/`sws.lst`/`sws.s` are a convenience snapshot,
not a cache.

## Schema gaps for this repo

1. Co-loaded program slot at 0x0F0000 / 0x0F0400 / 0x0FFE00.
   sws hardcodes a "shared memory protocol" with co-loaded
   binaries: command-in at 0x0F0000 (1 KB), output-out at
   0x0F0400 (4 KB), entry word at 0x0FFE00. The launcher schema
   needs a way to describe these as named slots distinct from
   "load this image" slots.

2. Patched entry-point table. Like sw-cor24-monitor, sws expects
   the emulator to publish an entry address into a fixed memory
   word via `--patch "0x0FFE00=0x..."` (editor-demo.sh:56). The
   schema needs a generic "post-load memory patch" list keyed
   by symbol from a sibling repo's listing file.

3. Symbol discovery from sibling .lst. The demo locates swye's
   `_main` by grepping `/tmp/swye.lst` (editor-demo.sh:35). The
   launcher schema may need a "discover symbol from listing"
   step, since absolute addresses can change with each build.

4. Multi-binary co-load with cross-repo source references. The
   demo loads four artifacts: sws.s (this repo), swye.bin
   (sibling sw-cor24-yocto-ed), a text file, and a keystroke
   binary. The launcher must accept (file, base_addr) pairs
   sourced from sibling repos.

5. Variable stack size. Editor demo bumps stack to 8K
   (`--stack-kilobytes 8`, editor-demo.sh:57). Per-run stack
   should be configurable.

6. UART input is interactive command text, not a source program.
   Unlike forth/basic, what sws reads from UART is shell command
   lines, not a program file. The schema's UART input field must
   distinguish "interactive REPL commands" from "source code to
   load."

7. Stub filesystem. Every `cd`/`ls`/`mkdir`/`source`-type command
   in sws expects an OS layer that does not yet exist. The
   launcher should not assume sws can read any file directly --
   only that it can invoke co-loaded binaries.

8. No relocatable program model. `run_resolve` (sws.c:1631-1636)
   knows only one name, "swye", and only one entry slot,
   `RUN_ENTRY_0`. Adding a second runnable binary requires a
   source change in sws.c, not just emulator flags. The schema
   should note this is a one-program-slot system in its current
   shape.
