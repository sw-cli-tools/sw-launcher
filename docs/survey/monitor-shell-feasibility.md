# Feasibility: monitor-based edit-and-run for other languages

The user asked, after surveying `sw-cor24-monitor`, `sw-cor24-script`
(sws), and `sw-cor24-yocto-ed`: how hard would it be for some of
the other languages (apl, basic, forth, ocaml, ...) to run inside
a resident monitor shell that can edit source and run it?

Short answer: forth and basic are nearly there already; apl is a
small lift; ocaml and the p-code-hosted languages need a bigger
rework because their build pipelines run host-side. The launcher
schema needs to express the *resident* run mode and the
*edit-and-run* trampoline as first-class concepts, but it does not
need to invent a new memory model -- the survey shows the patterns
already exist in three repos and they reuse the same
trampoline/shared-memory primitive.

## What the resident shell pattern actually requires

From the monitor / script / yocto-ed surveys, four primitives are
load-bearing:

1. **Service vector + trampoline** (`monitor.c`,
   `trampoline.s:14-69`). A small chunk of asm at a fixed address
   that saves SP, calls into a program by entry address, and
   accepts a non-local return via a `svc_exit` longjmp-style
   handler.
2. **Shared-memory protocol** (`script` 0x0F0000 / 0x0F0400 /
   0x0FFE00; `yocto-ed` 0x0F0000 / 0x0F0400; `monitor`
   `mon_run_request[32]`). Reserved memory regions for command
   text, output, and entry-address handoff between the resident
   shell and the program it spawns.
3. **Program slot table** (monitor's `mon_run_request` resolves
   names to entry addresses populated at boot). Lets the user
   type a program name; the shell looks it up; the trampoline
   jumps to the slot's entry.
4. **Pre-staged UART command stream** (yocto-ed's
   `SYE_CMD_ADDR=0x0F0000`). Lets the host stage canned input
   before falling through to live UART. Useful for tests; not
   load-bearing for interactive use.

Any language that wants edit-and-run needs an editor (yocto-ed
already exists) and a shell command that reads the editor buffer,
hands the source to a translator (assembler / compiler / parser),
hands the translated artifact to the trampoline, and reads UART /
shared-memory output.

## Per-language feasibility

### forth -- already there
forth is a REPL by design. Its outer interpreter reads UART, parses
words, and either compiles them into the dictionary or executes
them. "Edit and run" in forth is literally typing a colon
definition. To put forth *under* a monitor shell would mean having
the shell jump to forth's entry; forth runs until BYE; control
returns. forth's data stack (DSP), return stack (RSP), and
dictionary (HERE) all already live in non-overlapping regions, so
a `mon_run_request slot = forth` plus `--load-binary forth.bin@<slot>`
is enough. Effort: ~1 day to add forth as a registered program in
monitor, plus a "save dictionary as snapshot" word for persisting
edits between sessions. **Mostly schema work; almost no code.**

### basic -- one new word away
basic is also a REPL (interpreter loop in `src/basic.pas`). It
reads UART, accumulates a program in `PS=16384` chars, runs RUN /
LIST / NEW / etc. interactively. To put basic under a monitor
shell, monitor's program slot table just needs a `basic` entry
pointing at p24-load `pvm + basic.p24` (since basic itself runs as
p-code on pvm). The challenge is that the *current* basic image
embeds heaps that get reset on each invocation; preserving program
text across `monitor -> basic -> back to monitor -> basic` requires
either pinning the basic image's program area in shared memory
(point `PS` at `0x0F0400` instead of basic's BSS) or treating each
basic invocation as fresh. Effort: ~3 days to wire it up with
shared-memory persistence, ~1 day if each session is fresh.
**Schema needs `shared_regions` (gap F2) and per-slot program slot
entries.**

### apl
apl is harder than forth/basic because the apl interpreter is a
tinyc-compiled C program, not a REPL primitive. But the apl REPL
loop in `src/main.c` already reads UART line-by-line, accepts
`)OFF` as terminator, and uses fixed-size buffers. The
monitor-friendly variant is to (a) call apl as a registered
program, (b) reserve apl's `int heap[4096]` and `prog_buf` in
shared memory so program text survives a `monitor -> apl ->
monitor` round trip. Effort: ~3 days. **Same schema gaps as basic.**

### macrolisp
macrolisp already has a snapshot model (`prelude.snap` rehydrated
into heap_car / heap_cdr / symbols / strings). Resident-shell
integration is "save a snapshot on exit, restore on re-entry."
This is the cleanest fit of any of the higher-level languages. The
existing `snapshot-save.s` already runs on COR24; making it
consume the monitor's shared region instead of UART is a small
change. Effort: ~2 days. **Needs the schema's two-phase scenario
support (gap A5).**

### ocaml -- significant rework
The ocaml interpreter is Pascal-compiled to p-code, run on pvm,
fed source via UART, with heap living in a corridor between pvm
and ocaml.p24m. Putting it under a monitor shell requires:
- Loading pvm + ocaml.p24m as a registered slot (ok).
- Replacing UART source feed with reading from a shared-memory
  source buffer (the user-edited text from yocto-ed).
- Patching ocaml.p24m's getc primitive to read from the shared
  region rather than UART -- this is *interpreter-level* surgery,
  not just memory-map work.
- Persisting the OCaml heap across invocations (or accepting a
  fresh interpreter per run).
The Pascal source for the OCaml interpreter would need a
"source\_addr" patch alongside `code_ptr` and `heap_limit`. Effort:
~1-2 weeks. **Schema gap B2 (cross-layer symbol pulls) plus
interpreter-level changes.**

### pascal, plsw -- not interactive, big rework
pascal and plsw both compile-by-running-the-emulator: source goes
in over UART, assembled .s comes out via UART, host re-runs the
emulator with the assembled code. Putting *that* under a monitor
shell is conceptually possible but means the resident shell would
need to either (a) drive a second emulator instance internally
(impossible), or (b) host-side re-launch with the new image
(defeats the purpose of "resident"). Realistically these stay
host-driven; the monitor shell is for *running* their output, not
*invoking* them. Effort: not recommended.

### snobol4 -- composite-image already, edit-and-run not natural
snobol4 is a SNOBOL interpreter with input source loaded as a
binary at `0x080000` and runtime data optionally at `0x090000`.
Already monitor-shaped: the program reads source from a
known address, runs, prints output. To make it editable, the
yocto-ed buffer at `0x0F0400` (post-quit) just needs to be the
same address (or a redirector layer). Effort: ~3 days to add
yocto-ed handoff. **Schema gap F2 (shared regions) covers it.**

### smalltalk -- delegated, no direct path
smalltalk currently runs as host-compiled BASIC on top of basic-on-
pvm. There is no smalltalk runtime in COR24 memory; the whole
language exists at the BASIC layer. Edit-and-run for smalltalk
means edit-and-run for basic, plus the smalltalk -> BASIC compile
step running host-side. Not feasible without a native smalltalk
runtime, which is well out of scope.

## Cost summary

| language    | effort         | blockers                                  |
|-------------|----------------|-------------------------------------------|
| forth       | ~1 day         | mostly schema work                        |
| macrolisp   | ~2 days        | snapshot integration                      |
| basic       | ~3 days        | shared-memory program text                |
| apl         | ~3 days        | shared-memory program text                |
| snobol4     | ~3 days        | yocto-ed handoff                          |
| ocaml       | ~1-2 weeks     | interpreter-level getc rework             |
| pascal      | not recommended| compile-by-emulator pattern               |
| plsw        | not recommended| compile-by-emulator pattern               |
| smalltalk   | not feasible   | host-side compile chain                   |

## Implications for the launcher schema

Three concrete additions justified by this analysis (all already
listed in `schema-gaps.md`, restated here in priority order):

1. **`scenarios.<name>.run.mode = "batch" | "terminal" |
   "resident"`** (gap E3). The resident mode tells `sw-launch` not
   to wait for emulator exit and to drive UART interactively (or
   hand off to a TUI).

2. **`[layers.<n>.shared_regions]`** (gap F2). Named address
   ranges declared per layer, overlap-checked against every other
   layer's segments. This is what makes the trampoline /
   shared-memory protocol expressible without ad-hoc address
   literals scattered across scripts.

3. **`scenarios.<name>.programs = [{slot = "name", layer =
   "layerref"}, ...]`** -- a program slot table. Lets the launcher
   build the equivalent of monitor's `mon_run_request` registry
   from declarative config rather than from a hand-edited C array.

These three additions, plus the existing segment-aware overlap
check, are enough to express forth-in-monitor, basic-in-monitor,
apl-in-monitor, and macrolisp-in-monitor as scenarios. ocaml and
the compile-by-emulator pair (pascal, plsw) need source-side work
that is out of scope for the launcher itself.

## Recommended order of work (not a saga commitment)

Out-of-scope-for-this-step but recorded for later sagas:

1. Land schema additions 1 + 2 + 3 above as part of step
   002-revise-schema, with example TOMLs.
2. Land Phase 1 / Phase 2 / Phase 3 of the launcher as planned,
   testing with non-resident scenarios first.
3. Add a "resident" scenario shape to the launcher (Phase 4-ish)
   targeting forth-in-monitor as the smallest meaningful demo.
4. Add basic-in-monitor as the second demo; refine
   `shared_regions` ergonomics if needed.
5. apl, macrolisp, snobol4 follow quickly.
6. ocaml-in-monitor sits at the back of the queue and almost
   certainly wants to be its own saga with sw-cor24-ocaml repo
   contributions.
