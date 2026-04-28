# Survey: sw-cor24-monitor

This repo provides a resident COR24 monitor: it boots at address 0,
owns the UART hardware, exposes a 5-entry service vector at 0x500,
maintains a name -> entry-address program registry, and acts as a
synchronous "call/return" launcher for preloaded program binaries.
Unlike the one-shot run-script pattern (load runtime + image + UART
input, run, exit), the monitor stays resident and dispatches multiple
program invocations during one emulator run. It does NOT load programs
from UART; the emulator preloads every binary at a fixed address, and
the monitor only resolves names and jumps. The interactive UI is
delegated to a separate binary (sws, sw-cor24-script) loaded at
0x20000 -- the monitor calls sws in a loop, sws asks for programs to
run via a shared-memory protocol, and the monitor runs them on sws's
behalf.

## 1. Run scripts / entry points

`justfile` has the canonical recipes:
- `just build` -- compile monitor.c, prepend boot.s, append
  trampoline.s, write build/monitor.s
  (justfile lines 12-15).
- `just build-prog <src> <addr>` -- compile a user program with
  tc24r, strip its `_start`, prepend `src/prog_start.s`, assemble at
  base addr (justfile lines 18-24).
- `just run` -- emulator command line that loads four binaries:
  monitor.bin@0, echo.bin@0x2000, failtest.bin@0x3000, cat.bin@0x5000,
  entry 0 (justfile lines 67-68).
- `just demo-editor` -- runs `demos/monitor-editor-demo.sh`, the
  full-stack monitor -> sws -> swye demo.

The richer entry point is `demos/monitor-editor-demo.sh` (lines
108-120), which loads five binaries: monitor@0, sws@0x20000,
swye@0x40000, a textfile@0x10000, and pre-recorded keystrokes@0xF0000.
This is the load layout the launcher must be able to express.

## 2. Memory layout

From `docs/architecture.md` lines 5-24 (intent), `src/monitor.c`
lines 280-323 (actual addresses), and `src/boot.s` line 4 (stack):

| Region            | Address     | Notes                              |
|-------------------|-------------|------------------------------------|
| Monitor code      | 0x000000    | boot.s + monitor.c + trampoline.s  |
| Monitor data      | 0x000200    | (architecture.md, intent)          |
| Service vector    | 0x000500    | `svc_vector[5]` array              |
| sws shell        | 0x020000    | `mon_sws_entry` (monitor.c:323)    |
| Program slot       | 0x002000    | echo (monitor.c:314)               |
| Program slot       | 0x003000    | failtest (monitor.c:315)           |
| Program slot       | 0x005000    | cat (monitor.c:316)                |
| Program slot       | 0x040000    | swye editor (monitor.c:317)        |
| Stack top          | 0xFEEC00    | top of 3K EBR (boot.s:4)           |
| UART data          | 0xFF0100    | MMIO (monitor.c:26)                |
| UART status        | 0xFF0101    | MMIO (monitor.c:27)                |

Note the discrepancy: docs say sws lives at 0x1000 and program slots
are 4K, but the live code registers swye at 0x40000 and sws at
0x20000. The source is authoritative. The docs reflect an earlier,
tighter layout.

The monitor itself is small -- monitor.c is 335 lines, boot.s is
6 lines, trampoline.s is 92 lines, prog_start.s is 27 lines. All
pre-assembled into build/monitor.bin and loaded at 0.

## 3. Transfer of control

How a program gets received, placed, jumped to, and returns.

Receive: the emulator preloads every program binary with
`--load-binary file@addr`. The monitor never reads program code from
UART, never copies it, never relocates it. See
`demos/monitor-editor-demo.sh` lines 108-114 and
`justfile` lines 67-68.

Place: the user program lives at the address given to
`just build-prog`, e.g. 0x2000 for echo (justfile lines 27-30).
`mon_register("echo", 0x2000, 0)` records that name -> entry mapping
in the program table (`monitor.c` lines 314-317). The table holds up
to 16 entries (`prog_names[16]`, `prog_entries[16]` at monitor.c:15-17).

Jump: `mon_run(entry)` (`monitor.c` lines 235-241) calls the asm
routine `mon_invoke_program(entry, ctx)` where `ctx` is the address
of `svc_vector`. Inside `trampoline.s` lines 15-38:
1. Save monitor SP to `mon_saved_sp` (line 22-23) -- needed for the
   non-local-return path.
2. Push `ctx` as the program's first arg (line 28).
3. `jal r1,(r2)` jumps to entry, putting the return address in r1
   (line 30).
4. The program's `_start` (in `prog_start.s`, since tc24r's default
   `_start` was stripped) treats this as a normal function call:
   it forwards ctx to `main`, then returns via `jmp (r1)` (lines 11-27).
5. Trampoline regains control on line 31 with RC in r0, runs its
   epilogue, returns to mon_run.

Return path A (normal return): main returns int, prog_start's
epilogue jumps back to the trampoline, which returns RC to mon_run.

Return path B (non-local exit): a program that calls `vec[4](rc)`
hits `svc_exit_impl` (`trampoline.s` lines 47-69), which acts as
a longjmp: stash rc, restore SP from `mon_saved_sp`, run
mon_invoke_program's epilogue (pop r1/r2/fp), reload rc into r0,
`jmp (r1)`. This is how `exit7.c` returns 7 from inside a service
call without unwinding through main.

Top-level loop: `mon_sws_loop` (`monitor.c` lines 284-303). It
repeatedly invokes sws at 0x20000. If sws returns rc >= 256 it
treats this as a "run request": sws has written a program name into
the shared buffer `mon_run_request[32]` (monitor.c:22), monitor calls
`mon_run_by_name(mon_run_request)` and loops back to call sws again.
This is the resident-shell loop that distinguishes this repo from
the one-shot pipeline.

## 4. UART / interactive command set

Two distinct command surfaces:

Fallback shell (`mon_shell`, `monitor.c` lines 245-274) -- only
reached when sws is not loaded:
- `<name>` -- run registered program
- `list` -- list registered programs
- `help` -- show commands

There is no `G <addr>` go command, no `L` load command, no memory
inspect/deposit. You can only invoke programs that were registered
at boot time.

sws shell (separate repo) -- the real UI. Communicates with the
monitor by writing a program name into `mon_run_request` and
returning rc >= 256.

UART hardware: only the monitor touches MMIO at 0xFF0100/0xFF0101.
Programs use the service vector instead. The five services are
wired in `svc_init` (`monitor.c` lines 139-147), with slot 4
(`svc_exit`) overwritten by an asm impl after init.

## 5. Heaps and stacks

Single shared stack. SP is set once in `boot.s` line 4 to 0xFEEC00
(top of 3K EBR). The same stack is used by the monitor and by the
currently running program -- there is only ever one program active
at a time (synchronous call/return). Across invocations,
`mon_invoke_program` saves the monitor SP to `mon_saved_sp`
(`trampoline.s` line 22-23) so `svc_exit_impl` can restore it.

No heap. tc24r forbids malloc; everything is statically allocated.
Notable static buffers in `monitor.c`:
- `prog_names[16]`, `prog_entries[16]`, `prog_flags[16]` --
  registry, lines 15-17
- `mon_run_request[32]` -- shared name buffer for sws, line 22
- `mon_shell` `line[64]` -- input buffer, line 246
- `svc_vector[5]` -- service table, line 82

Stack size is 3K of EBR; the editor demo bumps this to 8K via
`--stack-kilobytes 8` (demo script line 117).

## 6. Build artifacts vs vendored

Everything in `build/` is generated by `just`. Listing:
- `monitor.s`, `monitor.bin`, `monitor.lst` -- the assembled monitor
- `monitor_c.s` -- tc24r output before concatenation with boot/tramp
- `prog_c.s`, `prog_body.s`, `prog.s`, `prog.bin`, `prog.lst` --
  scratch files for the last `build-prog` invocation
- `echo.bin/.lst`, `cat.bin/.lst`, `failtest.bin/.lst`,
  `ret42.bin/.lst`, `exit7.bin/.lst` -- per-program artifacts
- `sws_demo.{s,bin,lst}`, `swye_demo.{s,bin,lst}`,
  `swye_cmds.bin` -- artifacts from `demo-editor`, built from
  sibling repos sw-cor24-script and sw-cor24-yocto-ed

Nothing is vendored. The only inputs are:
- `src/*.s` and `src/monitor.c` in this repo
- tc24r (cross C compiler) on PATH
- `cor24-run` (emulator) on PATH
- For the demo: sibling repos at `../sw-cor24-script` and
  `../sw-cor24-yocto-ed`, found via relative path
  (`monitor-editor-demo.sh` lines 28-30)

There is no compiled-artifact cache; each invocation rebuilds via
`just build`.

## 7. Edit-and-run model

(a) Edit source inside the monitor: NO. There is no editor in the
monitor itself; `monitor.c` has no buffer, no parsing, no assembler.
The only input the monitor's fallback shell accepts is a program
name (`mon_shell`, monitor.c:245-274), and program names must be in
the registry, which is populated only at boot
(`monitor.c` lines 314-317).

(b) Load source from UART and assemble in place: NO. There is no
assembler, no compiler, and no UART code-load path in this repo.
`mon_run_by_name` (`monitor.c` lines 211-221) is purely a name
lookup followed by a jump to a preloaded address.

(c) Load pre-assembled .bin files: YES, but loading is done by the
emulator before the monitor even starts. The monitor's role is
limited to dispatching to addresses recorded in the registry. See
`justfile` line 68 (`--load-binary build/echo.bin@0x2000` etc.) and
`monitor.c:314` (`mon_register("echo", 0x2000, 0)`).

A separate "edit a buffer" capability exists in this stack -- the
swye editor is invoked as a registered program -- but it edits a
data buffer (a text file at 0x10000), not source code that gets
assembled and run. The keystrokes themselves are loaded as a
preloaded binary at 0xF0000 in the demo
(`monitor-editor-demo.sh` line 113).

Caching of compiled artifacts: NO. Every `just build` runs tc24r and
the assembler from scratch. There is no hash-keyed cache, no
incremental skip.

## Schema gaps for this repo

The monitor breaks several assumptions baked into single-binary
launchers:

1. Multi-binary load with named slots. The launcher must take a
   list of `(file, base_addr)` pairs, not a single image. Example
   from the editor demo: monitor@0, sws@0x20000, swye@0x40000,
   text@0x10000, keystrokes@0xF0000.

2. Resident process plus on-demand programs. Memory layout is not
   "code | image | uart-input" -- it is a fixed monitor at low
   memory, plus N program slots at fixed addresses, plus one or
   more data buffers. The schema needs a notion of "program slot:
   {name, addr, source_file}" distinct from "load a runtime image."

3. Entry point is not always 0. It is for cor24 (the CPU resets to
   0), but the launcher schema should still let other targets
   override entry. Here it is `--entry 0` (justfile line 68).

4. The runtime UI is itself a loaded binary, not a host-side script.
   The "shell" in this stack is sws, an entirely separate repo
   loaded at 0x20000. Schema must allow chaining: monitor delegates
   to sws which delegates back. A "run-script" abstraction that
   assumes one runtime binary cannot model this.

5. No "load source over UART" mode. Unlike e.g. the forth/basic
   surveys where UART input is source code, here UART input is
   command lines (program names) consumed by the resident shell.
   The schema's UART field needs to distinguish "source program
   text" from "interactive commands to a resident shell."

6. Shared-memory protocol between binaries. The monitor and sws
   communicate via `mon_run_request[32]` and rc >= 256 as a
   sentinel. The schema does not need to model this directly, but
   it should at least be aware that the two binaries share a memory
   plan.

7. Variable stack size. The editor demo bumps stack from 3K to 8K
   via `--stack-kilobytes 8`. Schema should expose stack size as
   per-run configurable.

8. Patch points. The demo uses `--patch 0x0FFE00=0x0${SWYE_MAIN}`
   to publish a discovered address into memory before run
   (`monitor-editor-demo.sh` line 114). Schema may need a generic
   "post-load memory patches" list.
